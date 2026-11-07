import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod';

// The collector is the credential authority. This bridge never receives device
// tokens, passwords, project keys, or raw session content.
const result = (text, isError = false) => ({ content: [{ type: 'text', text }], ...(isError ? { isError: true } : {}) });

export function createSetupServer({ executable = process.env.RROFLOW_MCP_COLLECTOR || 'rroflow', spawnProcess = spawn } = {}) {
let login = null;
let connecting = false;
function startLogin() {
  if (login?.state === 'pending') return Promise.resolve(result('A collector sign-in is already in progress. Call rroflow_setup_status.'));
  const child = spawnProcess(executable, ['login', '--auth-method', 'native'], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true, shell: false });
  const state = login = { state: 'pending', child, approval: null };
  child.stderr.resume();
  return new Promise(resolve => {
    let replied = false;
    const finish = value => { if (!replied) { replied = true; clearTimeout(timer); resolve(value); } };
    const timer = setTimeout(() => { state.state = 'failed'; child.kill(); finish(result('The collector did not provide an approval link. Check the deployment configuration and try again.', true)); }, 30000);
    child.on('error', () => { state.state = 'failed'; finish(result('Unable to start the RROFLOW collector. Install a compatible collector and check its public deployment configuration.', true)); });
    child.on('close', code => { state.state = code === 0 ? 'approved' : 'failed'; if (!replied) finish(result('Collector sign-in could not start. Check that native device sign-in is enabled and the deployment profile is configured.', true)); });
    createInterface({ input: child.stdout }).on('line', line => {
      const match = /^To connect this collector, open (\S+) and enter the code ([A-Z2-9]{5}-[A-Z2-9]{5})$/.exec(line);
      if (!match || replied || state.state !== 'pending') return;
      let link;
      try { link = new URL(match[1]); } catch { return; }
      if (link.protocol !== 'https:' || link.username || link.password || link.pathname !== '/app/device' || link.search || link.hash) return;
      state.approval = { link: link.href, code: match[2] };
      finish(result(`Open ${link.href} and enter code ${match[2]}. Sign in and choose your project. Then call rroflow_setup_status. The collector keeps its credential locally.`));
    });
  });
}

const server = new McpServer({ name: 'rroflow-setup', version: '0.0.0' });
server.registerTool('rroflow_setup_start', {
  description: 'Start native collector device sign-in and return the approval page and short code. Requires an installed collector and configured deployment profile.',
  inputSchema: {}, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true }
}, async () => startLogin());
server.registerTool('rroflow_setup_status', {
  description: 'Check this setup bridge’s in-progress collector sign-in. Approval does not yet install capture or prove events are delivered.',
  inputSchema: {}, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false }
}, async () => result(!login ? 'No sign-in started.' : login.state === 'pending' ? 'Waiting for device approval.' : login.state === 'approved' ? 'Device sign-in completed. Capture has not yet been linked or verified.' : 'Device sign-in failed. Check the collector configuration and sign-in availability.'));
server.registerTool('rroflow_sources_connect', {
  description: 'After device sign-in, link the local Claude Code and Codex session folders, including existing history and future sessions, to the single project selected during approval, and register the collector to start at login. This does not route repositories to separate projects. Only call after the person explicitly agrees to this scope. Content is off by default.',
  inputSchema: {
    confirm_history_and_startup: z.literal(true).describe('The person explicitly agrees to link past and future local Claude Code/Codex sessions to the selected project and enable the collector at login.'),
    include_content: z.boolean().default(false).describe('Include scrubbed prompt, response and tool text only with the person’s explicit additional consent.')
  }, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true }
}, async ({ include_content }) => {
  if (login?.state !== 'approved') return result('Complete device approval in this setup session before linking sources.', true);
  if (connecting) return result('Source setup is already running. Wait for its result before retrying.', true);
  connecting = true;
  const args = ['setup', '--yes', ...(include_content ? ['--content'] : [])];
  let child;
  try { child = spawnProcess(executable, args, { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true, shell: false }); }
  catch { connecting = false; return result('Unable to run source setup. Check the collector locally.', true); }
  child.stdout.resume(); child.stderr.resume();
  return await new Promise(resolve => {
    let finished = false;
    const finish = value => { if (!finished) { finished = true; connecting = false; clearTimeout(timer); resolve(value); } };
    const timer = setTimeout(() => { child.kill(); finish(result('Source setup timed out. Check the collector locally before retrying; setup may have partially completed.', true)); }, 90000);
    child.on('error', () => finish(result('Unable to run source setup. Check the collector locally.', true)));
    child.on('close', code => finish(code === 0
      ? result('Source setup completed. Open the project connection details to verify that a real event arrived; setup alone does not prove delivery.')
      : result('Source setup did not complete. Check collector status locally; existing queues were not cleared by this bridge.', true)));
  });
});
return server;
}
