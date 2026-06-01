import { z } from "zod";
import { Tool } from "../Tool";
import "reflect-metadata";
import type { ToolResponse } from "../ToolResponse";
import { TextResponse } from "../ToolResponse";
import { LogosMcpServer } from "../LogosMcpServer";
import { ApiDocs } from "../ApiDocs";

/**
 * Arguments class for the LogosApiInfoTool
 */
export class LogosApiInfoArgs {
    static schema = {
        type: z.string().min(1, "Type name cannot be empty"),
        member: z.string().optional(),
    };

    /**
     * The API type name to retrieve information for.
     */
    type!: string;

    /**
     * The specific member name to retrieve (optional).
     */
    member?: string;
}

/**
 * Tool for retrieving Logos API documentation information.
 *
 * This tool provides access to API type documentation loaded from YAML files,
 * allowing retrieval of either full type documentation or specific member details.
 */
export class LogosApiInfoTool extends Tool<LogosApiInfoArgs> {
    private static readonly MAX_FULL_TEXT_CHARS = 2000;
    private readonly apiDocs: ApiDocs;

    /**
     * Creates a new LogosApiInfo tool instance.
     *
     * @param mcpServer - The MCP server instance
     */
    constructor(mcpServer: LogosMcpServer, apiDocs: ApiDocs) {
        super(mcpServer, LogosApiInfoArgs.schema);
        this.apiDocs = apiDocs;
    }

    public getToolName(): string {
        return "logos_api_info";
    }

    public getToolDescription(): string {
        return (
            "Retrieves Logos API documentation for types and their members." +
            "Be sure to read the 'Logos High-Level Overview' first."
        );
    }

    protected async executeCore(args: LogosApiInfoArgs): Promise<ToolResponse> {
        const apiType = this.apiDocs.getType(args.type);

        if (!apiType) {
            throw new Error(`API type "${args.type}" not found`);
        }

        if (args.member) {
            // return specific member documentation
            const memberDoc = apiType.getMember(args.member);
            if (!memberDoc) {
                throw new Error(`Member "${args.member}" not found in type "${args.type}"`);
            }
            return new TextResponse(memberDoc);
        } else {
            // return full text or overview based on length
            const fullText = apiType.getFullText();
            if (fullText.length <= LogosApiInfoTool.MAX_FULL_TEXT_CHARS) {
                return new TextResponse(fullText);
            } else {
                return new TextResponse(
                    apiType.getOverviewText() +
                        "\n\nMember details not provided (too long). " +
                        "Call this tool with a member name for more information."
                );
            }
        }
    }
}
