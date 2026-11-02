import { timingSafeEqual } from "node:crypto";
import { isIP } from "node:net";

export interface LocalMcpSecurityConfig {
    host: string;
    token: string;
}

export function loadLocalMcpSecurityConfig(
    env: NodeJS.ProcessEnv = process.env,
    multiUserMode = false
): LocalMcpSecurityConfig {
    if (multiUserMode || env.LOGOS_MCP_REMOTE_MODE === "true") {
        throw new Error("Remote and multi-user MCP modes are disabled pending separate security approval");
    }

    const host = env.LOGOS_MCP_SERVER_HOST ?? env.LOGOS_MCP_SERVER_LISTEN_ADDRESS ?? "127.0.0.1";
    if (!isLoopbackHost(host)) {
        throw new Error("MCP listeners may bind only to 127.0.0.1 or ::1");
    }

    const token = env.LOGOS_MCP_LOCAL_TOKEN ?? "";
    if (Buffer.byteLength(token, "utf8") < 32) {
        throw new Error("LOGOS_MCP_LOCAL_TOKEN must contain at least 32 bytes");
    }

    return { host, token };
}

export function isLoopbackHost(host: string): boolean {
    return host === "::1" || (host === "127.0.0.1" && isIP(host) === 4);
}

export function hasValidLocalToken(candidate: string | undefined, expected: string): boolean {
    if (!candidate || !expected) return false;

    const candidateBytes = Buffer.from(candidate, "utf8");
    const expectedBytes = Buffer.from(expected, "utf8");
    return candidateBytes.length === expectedBytes.length && timingSafeEqual(candidateBytes, expectedBytes);
}

export function hasValidBearerHeader(header: string | undefined, expected: string): boolean {
    if (!header) return false;
    const match = /^Bearer\s+(.+)$/i.exec(header);
    return match !== null && hasValidLocalToken(match[1], expected);
}
