import assert from "node:assert/strict";
import test from "node:test";
import { hasValidBearerHeader, hasValidLocalToken, loadLocalMcpSecurityConfig } from "./localSecurity";

const token = "a".repeat(64);

test("local MCP defaults bind only to loopback and require a strong token", () => {
    assert.deepEqual(loadLocalMcpSecurityConfig({ LOGOS_MCP_LOCAL_TOKEN: token }), {
        host: "127.0.0.1",
        token,
    });
    assert.throws(() => loadLocalMcpSecurityConfig({}), /LOGOS_MCP_LOCAL_TOKEN/);
    assert.throws(
        () => loadLocalMcpSecurityConfig({ LOGOS_MCP_LOCAL_TOKEN: token, LOGOS_MCP_SERVER_HOST: "0.0.0.0" }),
        /127\.0\.0\.1 or ::1/
    );
    assert.throws(
        () => loadLocalMcpSecurityConfig({ LOGOS_MCP_LOCAL_TOKEN: token, LOGOS_MCP_REMOTE_MODE: "true" }),
        /Remote and multi-user/
    );
    assert.throws(() => loadLocalMcpSecurityConfig({ LOGOS_MCP_LOCAL_TOKEN: token }, true), /disabled/);
});

test("local token and bearer checks reject missing or incorrect credentials", () => {
    assert.equal(hasValidLocalToken(token, token), true);
    assert.equal(hasValidLocalToken("b".repeat(64), token), false);
    assert.equal(hasValidLocalToken(undefined, token), false);
    assert.equal(hasValidBearerHeader(`Bearer ${token}`, token), true);
    assert.equal(hasValidBearerHeader(`bearer ${token}`, token), true);
    assert.equal(hasValidBearerHeader(token, token), false);
    assert.equal(hasValidBearerHeader(`Bearer ${"b".repeat(64)}`, token), false);
});
