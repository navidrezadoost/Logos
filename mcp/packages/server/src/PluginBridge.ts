import { WebSocket, WebSocketServer } from "ws";
import { PluginTask } from "./PluginTask";
import { PluginTaskResponse, PluginTaskResult } from "@logos/mcp-common";
import { createLogger } from "./logger";
import type { LogosMcpServer } from "./LogosMcpServer";

interface ClientConnection {
    socket: WebSocket;
}

/**
 * Manages WebSocket connections to Logos plugin instances and handles plugin tasks
 * over these connections.
 */
export class PluginBridge {
    private readonly logger = createLogger("PluginBridge");
    private readonly wsServer: WebSocketServer;
    private readonly connectedClients: Map<WebSocket, ClientConnection> = new Map();
    private readonly pendingTasks: Map<string, PluginTask<any, any>> = new Map();
    private readonly taskTimeouts: Map<string, NodeJS.Timeout> = new Map();

    constructor(
        public readonly mcpServer: LogosMcpServer,
        private port: number,
        private taskTimeoutSecs: number = 30
    ) {
        this.wsServer = new WebSocketServer({ port, host: mcpServer.host });
        this.setupWebSocketHandlers();
    }

    /**
     * Sets up WebSocket connection handlers for plugin communication.
     *
     * Manages client connections and provides bidirectional communication
     * channel between the MCP mcpServer and Logos plugin instances.
     */
    private setupWebSocketHandlers(): void {
        this.wsServer.on("connection", (ws: WebSocket) => {
            let authenticated = false;
            this.logger.info("WebSocket connection opened; awaiting local authentication");
            const authTimeout = setTimeout(() => {
                if (!authenticated && ws.readyState === WebSocket.OPEN) {
                    ws.close(1008, "Local authentication required");
                }
            }, 5000);

            ws.on("message", (data) => {
                try {
                    const messageText = Buffer.isBuffer(data)
                        ? data.toString("utf8")
                        : Array.isArray(data)
                          ? Buffer.concat(data).toString("utf8")
                          : Buffer.from(data as ArrayBuffer).toString("utf8");
                    const message = JSON.parse(messageText);
                    if (!authenticated) {
                        if (
                            message?.type !== "logos.mcp.auth" ||
                            !this.mcpServer.isLocalTokenValid(message?.token)
                        ) {
                            ws.close(1008, "Invalid local authentication");
                            return;
                        }

                        authenticated = true;
                        clearTimeout(authTimeout);
                        this.connectedClients.set(ws, { socket: ws });
                        ws.send(JSON.stringify({ type: "logos.mcp.auth", ok: true }));
                        this.logger.info("Authenticated local plugin WebSocket connection");
                        return;
                    }

                    const response: PluginTaskResponse<any> = message;
                    this.handlePluginTaskResponse(response);
                } catch (error) {
                    this.logger.warn(error, "Rejected malformed WebSocket message");
                    if (!authenticated && ws.readyState === WebSocket.OPEN) {
                        ws.close(1008, "Invalid local authentication");
                    }
                }
            });

            ws.on("close", () => {
                clearTimeout(authTimeout);
                this.logger.info("WebSocket connection closed");
                this.connectedClients.delete(ws);
            });

            ws.on("error", (error) => {
                this.logger.error(error, "WebSocket connection error");
                clearTimeout(authTimeout);
                this.connectedClients.delete(ws);
            });
        });

        this.logger.info("WebSocket mcpServer started on port %d", this.port);
    }

    /**
     * Handles responses from the plugin for completed tasks.
     *
     * Finds the pending task by ID and resolves or rejects its promise
     * based on the execution result.
     *
     * @param response - The plugin task response containing ID and result
     */
    private handlePluginTaskResponse(response: PluginTaskResponse<any>): void {
        const task = this.pendingTasks.get(response.id);
        if (!task) {
            this.logger.info(`Received response for unknown task ID: ${response.id}`);
            return;
        }

        // Clear the timeout and remove the task from pending tasks
        const timeoutHandle = this.taskTimeouts.get(response.id);
        if (timeoutHandle) {
            clearTimeout(timeoutHandle);
            this.taskTimeouts.delete(response.id);
        }
        this.pendingTasks.delete(response.id);

        // Resolve or reject the task's promise based on the result
        if (response.success) {
            task.resolveWithResult({ data: response.data });
        } else {
            const error = new Error(response.error || "Task execution failed (details not provided)");
            task.rejectWithError(error);
        }

        this.logger.info(`Task ${response.id} completed: success=${response.success}`);
    }

    /**
     * Determines the client connection to use for executing a task.
     *
     * Returns the single authenticated local plugin connection.
     *
     * @returns The client connection to use
     * @throws Error if no suitable connection is found or if configuration is invalid
     */
    private getClientConnection(): ClientConnection {
        if (this.connectedClients.size === 0) {
            throw new Error(
                "No authenticated Logos plugin is connected. Authenticate the local plugin connection first."
            );
        }

        if (this.connectedClients.size > 1) {
            throw new Error("More than one authenticated local plugin is connected; disconnect extras first.");
        }

        return this.connectedClients.values().next().value as ClientConnection;
    }

    public async stop(): Promise<void> {
        for (const client of this.wsServer.clients) {
            client.close(1001, "MCP server shutting down");
        }
        await new Promise<void>((resolve) => this.wsServer.close(() => resolve()));
    }

    /**
     * Executes a plugin task by sending it to connected clients.
     *
     * Registers the task for result correlation and returns a promise
     * that resolves when the plugin responds with the execution result.
     *
     * @param task - The plugin task to execute
     * @throws Error if no plugin instances are connected or available
     */
    public async executePluginTask<TResult extends PluginTaskResult<any>>(
        task: PluginTask<any, TResult>
    ): Promise<TResult> {
        // get the appropriate client connection based on mode
        const connection = this.getClientConnection();

        // register the task for result correlation
        this.pendingTasks.set(task.id, task);

        // send task to the selected client
        const requestMessage = JSON.stringify(task.toRequest());
        if (connection.socket.readyState !== 1) {
            // WebSocket is not open
            this.pendingTasks.delete(task.id);
            throw new Error(`Plugin instance is disconnected. Task could not be sent.`);
        }

        connection.socket.send(requestMessage);

        // Set up a timeout to reject the task if no response is received
        const timeoutHandle = setTimeout(() => {
            const pendingTask = this.pendingTasks.get(task.id);
            if (pendingTask) {
                this.pendingTasks.delete(task.id);
                this.taskTimeouts.delete(task.id);
                pendingTask.rejectWithError(
                    new Error(`Task ${task.id} timed out after ${this.taskTimeoutSecs} seconds`)
                );
            }
        }, this.taskTimeoutSecs * 1000);

        this.taskTimeouts.set(task.id, timeoutHandle);
        this.logger.info(`Sent task ${task.id} to connected client`);

        return await task.getResultPromise();
    }
}
