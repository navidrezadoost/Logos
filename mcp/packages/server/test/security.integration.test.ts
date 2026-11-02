import assert from "node:assert/strict";
import { createServer } from "node:net";
import test from "node:test";
import { WebSocket } from "ws";
import { LogosMcpServer } from "../src/LogosMcpServer";

const token = "local-test-token-0123456789abcdef0123456789abcdef";

async function unusedLoopbackPort(): Promise<number> {
    const server = createServer();
    await new Promise<void>((resolve, reject) => {
        server.once("error", reject);
        server.listen(0, "127.0.0.1", () => resolve());
    });
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("could not resolve test port");
    const { port } = address;
    await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
    return port;
}

function waitForOpen(socket: WebSocket): Promise<void> {
    return new Promise((resolve, reject) => {
        socket.once("open", resolve);
        socket.once("error", reject);
    });
}

function waitForMessage(socket: WebSocket): Promise<Buffer> {
    return new Promise((resolve, reject) => {
        socket.once("message", (data) => resolve(Buffer.isBuffer(data) ? data : Buffer.from(data)));
        socket.once("error", reject);
    });
}

function waitForClose(socket: WebSocket): Promise<number> {
    return new Promise((resolve, reject) => {
        socket.once("close", (code) => resolve(code));
        socket.once("error", reject);
    });
}

test("all local MCP transports require authentication and cannot bind off-loopback", async () => {
    const previous = {
        token: process.env.LOGOS_MCP_LOCAL_TOKEN,
        host: process.env.LOGOS_MCP_SERVER_HOST,
        httpPort: process.env.LOGOS_MCP_SERVER_PORT,
        wsPort: process.env.LOGOS_MCP_WEBSOCKET_PORT,
        replPort: process.env.LOGOS_MCP_REPL_PORT,
        remote: process.env.LOGOS_MCP_REMOTE_MODE,
    };

    const [httpPort, wsPort, replPort] = await Promise.all([
        unusedLoopbackPort(),
        unusedLoopbackPort(),
        unusedLoopbackPort(),
    ]);
    process.env.LOGOS_MCP_LOCAL_TOKEN = token;
    process.env.LOGOS_MCP_SERVER_HOST = "127.0.0.1";
    process.env.LOGOS_MCP_SERVER_PORT = String(httpPort);
    process.env.LOGOS_MCP_WEBSOCKET_PORT = String(wsPort);
    process.env.LOGOS_MCP_REPL_PORT = String(replPort);
    delete process.env.LOGOS_MCP_REMOTE_MODE;

    const server = new LogosMcpServer();
    const base = `http://127.0.0.1:${httpPort}`;
    const replBase = `http://127.0.0.1:${replPort}`;
    const wsUrl = `ws://127.0.0.1:${wsPort}`;
    try {
        assert.equal(server.host, "127.0.0.1");
        await server.start();

        const unauthenticatedMcp = await fetch(`${base}/mcp`, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ method: "tools/list" }),
        });
        assert.equal(unauthenticatedMcp.status, 401);
        await unauthenticatedMcp.body?.cancel();

        const wrongBearerMcp = await fetch(`${base}/mcp?userToken=${token}`, {
            method: "POST",
            headers: {
                authorization: `Bearer ${"b".repeat(64)}`,
                "content-type": "application/json",
            },
            body: JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }),
        });
        assert.equal(wrongBearerMcp.status, 401);
        await wrongBearerMcp.body?.cancel();

        const authenticatedMcp = await fetch(`${base}/mcp`, {
            method: "POST",
            headers: {
                authorization: `Bearer ${token}`,
                "content-type": "application/json",
                accept: "application/json, text/event-stream",
            },
            body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list", params: {} }),
        });
        assert.notEqual(authenticatedMcp.status, 401);
        await authenticatedMcp.body?.cancel();

        const unauthenticatedRepl = await fetch(`${replBase}/execute`, { method: "POST" });
        assert.equal(unauthenticatedRepl.status, 401);
        await unauthenticatedRepl.body?.cancel();
        const authenticatedRepl = await fetch(`${replBase}/execute`, {
            method: "POST",
            headers: {
                authorization: `Bearer ${token}`,
                "content-type": "application/json",
            },
            body: "{}",
        });
        assert.equal(authenticatedRepl.status, 400);
        await authenticatedRepl.body?.cancel();

        const unauthenticatedPairing = await fetch(`${base}/local/plugin-pairing`, { method: "POST" });
        assert.equal(unauthenticatedPairing.status, 401);
        await unauthenticatedPairing.body?.cancel();

        const pairingResponse = await fetch(`${base}/local/plugin-pairing`, {
            method: "POST",
            headers: { authorization: `Bearer ${token}` },
        });
        assert.equal(pairingResponse.status, 201);
        assert.equal(pairingResponse.headers.get("cache-control"), "no-store");
        const pairing = (await pairingResponse.json()) as { code: string; expiresInSeconds: number };
        assert.equal(pairing.expiresInSeconds, 60);

        const unauthorizedSocket = new WebSocket(wsUrl);
        await waitForOpen(unauthorizedSocket);
        const rejectedSocket = waitForClose(unauthorizedSocket);
        unauthorizedSocket.send(JSON.stringify({ id: "forged-task", success: true, data: {} }));
        const closeCode = await rejectedSocket;
        assert.equal(closeCode, 1008);

        const invalidSocket = new WebSocket(wsUrl);
        await waitForOpen(invalidSocket);
        const invalidClose = waitForClose(invalidSocket);
        invalidSocket.send(JSON.stringify({ type: "logos.mcp.auth", pairingCode: "not-a-pairing-code" }));
        assert.equal(await invalidClose, 1008);

        const authenticatedSocket = new WebSocket(wsUrl);
        await waitForOpen(authenticatedSocket);
        const authenticationAck = waitForMessage(authenticatedSocket);
        authenticatedSocket.send(JSON.stringify({ type: "logos.mcp.auth", pairingCode: pairing.code }));
        const ack = await authenticationAck;
        assert.deepEqual(JSON.parse(ack.toString()), { type: "logos.mcp.auth", ok: true });
        const socketClosed = waitForClose(authenticatedSocket);
        authenticatedSocket.close();
        await socketClosed;

        const replayedPairingSocket = new WebSocket(wsUrl);
        await waitForOpen(replayedPairingSocket);
        const replayedPairingClose = waitForClose(replayedPairingSocket);
        replayedPairingSocket.send(JSON.stringify({ type: "logos.mcp.auth", pairingCode: pairing.code }));
        assert.equal(await replayedPairingClose, 1008);
    } finally {
        await server.stop();
        await assert.rejects(fetch(`${base}/mcp`));
        await assert.rejects(fetch(`${replBase}/`));
        const stoppedSocket = new WebSocket(wsUrl);
        await assert.rejects(waitForOpen(stoppedSocket));
        if (previous.token === undefined) delete process.env.LOGOS_MCP_LOCAL_TOKEN;
        else process.env.LOGOS_MCP_LOCAL_TOKEN = previous.token;
        if (previous.host === undefined) delete process.env.LOGOS_MCP_SERVER_HOST;
        else process.env.LOGOS_MCP_SERVER_HOST = previous.host;
        if (previous.httpPort === undefined) delete process.env.LOGOS_MCP_SERVER_PORT;
        else process.env.LOGOS_MCP_SERVER_PORT = previous.httpPort;
        if (previous.wsPort === undefined) delete process.env.LOGOS_MCP_WEBSOCKET_PORT;
        else process.env.LOGOS_MCP_WEBSOCKET_PORT = previous.wsPort;
        if (previous.replPort === undefined) delete process.env.LOGOS_MCP_REPL_PORT;
        else process.env.LOGOS_MCP_REPL_PORT = previous.replPort;
        if (previous.remote === undefined) delete process.env.LOGOS_MCP_REMOTE_MODE;
        else process.env.LOGOS_MCP_REMOTE_MODE = previous.remote;
    }
});
