import { spawn, execFile } from 'node:child_process';
import { createInterface } from 'node:readline';
import { randomUUID } from 'node:crypto';
import { promisify } from 'node:util';

const exec = promisify(execFile);

/** A Rust collector subprocess acknowledges fsynced events, not network delivery. */
export class Capture {
  #child; #pending = []; #done; #failure; #tail = Promise.resolve(); #sequence = 0; #closed = false; #content = false;
  constructor({ agent, sessionId, devspace, spool, endpoint, executable = 'rroflow', content = false }) {
    if (!['codex', 'claude', 'grok'].includes(agent)) throw new Error('Unsupported capture agent');
    if (![sessionId, devspace, spool].every(v => typeof v === 'string' && v.length)) throw new Error('sessionId, devspace and spool are required');
    // Never silently bind a durable outbox to someone else's hosted service.
    let destination;
    try { destination = new URL(endpoint); } catch { throw new Error('An explicit ingest endpoint is required'); }
    if (typeof endpoint !== 'string' || endpoint !== endpoint.trim() || /[\\\s?#]/.test(endpoint)
      || destination.username || destination.password || destination.pathname !== '/'
      || !(destination.protocol === 'https:' || (destination.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(destination.hostname)))) {
      throw new Error('Endpoint must be an HTTPS origin without credentials, path, query or fragment (loopback HTTP allowed)');
    }
    this.#content = content === true;
    this.agent = agent; this.sessionId = sessionId; this.captureId = randomUUID();
    this.executable = executable; this.spool = spool;
    this.#child = spawn(executable, ['capture', 'record', '--spool', spool, '--endpoint', endpoint,
      '--devspace', devspace, ...(this.#content ? ['--content'] : [])], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, shell: false });
    let diagnostic = '';
    this.#child.stderr.setEncoding('utf8');
    this.#child.stderr.on('data', s => { diagnostic = (diagnostic + s).slice(-4096); });
    const fail = error => { this.#failure = error; for (const p of this.#pending.splice(0)) p.reject(error); };
    this.#child.on('error', fail);
    this.#child.stdin.on('error', fail);
    createInterface({ input: this.#child.stdout }).on('line', line => {
      const pending = this.#pending.shift();
      if (!pending) return fail(new Error('Unsolicited collector acknowledgement'));
      try {
        const ack = JSON.parse(line);
        if (!ack.durable || ack.event_id !== pending.id) throw new Error('Collector did not acknowledge the expected event');
        pending.resolve(ack);
      } catch (e) { pending.reject(e); fail(e); }
    });
    this.#done = new Promise((resolve, reject) => {
      this.#child.once('error', reject);
      this.#child.once('close', code => {
        if (code === 0 && this.#pending.length === 0) resolve();
        else { const error = new Error(`Capture recorder stopped (${code}): ${diagnostic.trim()}`); fail(error); reject(error); }
      });
    });
    // The caller observes errors via record()/close(); avoid an unhandled rejection
    // when a missing executable fails before their first record call.
    this.#done.catch(() => {});
  }
  record(kind, { operationId, parentOperationId = null, nativeType = null, payload = null } = {}) {
    if (this.#closed) return Promise.reject(new Error('Capture is closed'));
    const u = payload?.usage || payload?.response?.usage || payload?.message?.usage || payload?.event?.message?.usage || payload?.event?.usage;
    const n = value => Number.isSafeInteger(value) && value >= 0 ? value : null;
    const usage = u ? { input_tokens: n(u.input_tokens), output_tokens: n(u.output_tokens),
      cache_read_tokens: n(u.cache_read_input_tokens ?? u.cached_input_tokens ?? u.input_tokens_details?.cached_tokens),
      cache_write_tokens: n(u.cache_creation_input_tokens ?? u.cache_write_input_tokens), input_includes_cache: this.agent !== 'claude' } : null;
    const model = payload?.model || payload?.response?.model || payload?.message?.model || payload?.event?.message?.model || null;
    const block = payload?.event?.content_block;
    const native_id = payload?.item?.id || payload?.response?.id || payload?.tool_use_id || payload?.uuid || block?.id || null;
    const parent_native_id = payload?.parent_tool_use_id || payload?.parent_id || null;
    const native_status = payload?.item?.status || payload?.response?.status || payload?.subtype || null;
    const tool_name = payload?.tool_name || payload?.item?.name || block?.name || null;
    const exit_code = Number.isInteger(payload?.item?.exit_code) ? payload.item.exit_code : null;
    const event = { schema: 'rroflow.capture/v1', event_id: randomUUID(), capture_id: this.captureId,
      sequence: this.#sequence++, agent: this.agent, session_id: this.sessionId,
      operation_id: operationId || randomUUID(), parent_operation_id: parentOperationId,
      observed_at: new Date().toISOString(), kind, native_type: nativeType, native_id, parent_native_id, native_status, tool_name, exit_code, model, usage, payload: this.#content ? payload : null };
    const task = this.#tail.then(() => new Promise((resolve, reject) => {
      if (this.#failure) return reject(this.#failure);
      let line; try { line = JSON.stringify(event) + '\n'; } catch (e) { return reject(e); }
      this.#pending.push({ id: event.event_id, resolve, reject });
      this.#child.stdin.write(line, error => { if (error) { this.#failure = error; reject(error); } });
    }));
    // A recorder may reject its destination before reading stdin. On Linux the
    // pipe can break before stderr/close arrives; preserve that refusal instead
    // of hiding it behind EPIPE. A bounded wait also covers a stuck subprocess.
    const observed = task.catch(async error => {
      if (!['EPIPE', 'EOF', 'ERR_STREAM_DESTROYED'].includes(error?.code)) throw error;
      let timer;
      try {
        await Promise.race([this.#done, new Promise((_, reject) => {
          timer = setTimeout(() => reject(error), 1000);
        })]);
      } finally { clearTimeout(timer); }
      throw error;
    });
    this.#tail = observed;
    return observed;
  }
  /** Record the observable stream before handing each event to its consumer. */
  async *stream(create, { input = null, operationId = randomUUID(), parentOperationId = null } = {}) {
    const scope = { operationId, parentOperationId };
    await this.record('call.started', { ...scope, payload: input });
    let finished = false;
    try {
      const iterable = await create();
      for await (const event of iterable) {
        const type = event?.type || event?.object || 'unknown';
        // Unknown event types are journaled as well; there is no classifier filter.
        await this.record('stream.event', { ...scope, nativeType: type, payload: event });
        yield event;
      }
      await this.record('stream.completed', scope);
      finished = true;
    } catch (error) {
      // If durable capture itself failed, fail closed instead of claiming coverage.
      if (this.#failure) throw error;
      await this.record('call.failed', { ...scope, payload: { name: error?.name, message: String(error?.message || error) } });
      finished = true;
      throw error;
    } finally {
      if (!finished && !this.#failure) await this.record('stream.cancelled', scope);
    }
  }
  async call(run, { input = null, operationId = randomUUID(), parentOperationId = null } = {}) {
    const scope = { operationId, parentOperationId };
    await this.record('call.started', { ...scope, payload: input });
    let result;
    try { result = await run(); }
    catch (error) {
      await this.record('call.failed', { ...scope, payload: { name: error?.name, message: String(error?.message || error) } });
      throw error;
    }
    await this.record('call.completed', { ...scope, payload: result });
    return result;
  }
  /** Wrap an actual client-side tool (a model's proposed tool call is not execution). */
  async tool(name, run, { input = null, operationId = randomUUID(), parentOperationId = null } = {}) {
    const scope = { operationId, parentOperationId, nativeType: name };
    await this.record('tool.started', { ...scope, payload: input });
    let result;
    try { result = await run(); }
    catch (error) {
      await this.record('tool.failed', { ...scope, payload: { name: error?.name, message: String(error?.message || error) } });
      throw error;
    }
    await this.record('tool.completed', { ...scope, payload: result });
    return result;
  }
  /** Flush explicitly; failure leaves every unacknowledged record queued. */
  async flush() {
    await this.#tail;
    const { stdout } = await exec(this.executable, ['capture', 'flush', '--spool', this.spool], { windowsHide: true, maxBuffer: 65536 });
    return JSON.parse(stdout);
  }
  async close({ flush = true } = {}) {
    if (this.#closed) return;
    this.#closed = true;
    try { await this.#tail; }
    finally { this.#child.stdin.end(); }
    await this.#done;
    if (flush) return this.flush();
  }
}

/** Codex TypeScript runStreamed() exposes its native thread/turn/item events. */
export function codexStream(capture, thread, input, options) {
  return capture.stream(async () => (await thread.runStreamed(input, options)).events, { input });
}
/** Opt into Claude's partial events; preserve caller hooks and all other options. */
export function claudeStream(capture, query, args) {
  const operationId = randomUUID();
  const hooks = { ...args.options?.hooks };
  const names = ['PreToolUse', 'PostToolUse', 'PostToolUseFailure', 'UserPromptSubmit', 'Stop', 'SubagentStart', 'SubagentStop', 'PreCompact', 'PermissionRequest', 'SessionStart', 'SessionEnd', 'Notification'];
  for (const name of names) {
    const observe = async (input, toolUseID) => {
      await capture.record('hook.event', { operationId: toolUseID || operationId,
        parentOperationId: toolUseID ? operationId : null, nativeType: name, payload: input });
      return {}; // Observation only: preserve the caller's permission decisions.
    };
    hooks[name] = [...(hooks[name] || []), { hooks: [observe] }];
  }
  return capture.stream(() => query({ ...args, options: { ...args.options, hooks, includePartialMessages: true } }), { input: args.prompt, operationId });
}
/** Grok through xAI's OpenAI-compatible Responses API, using the caller's client. */
export function grokStream(capture, client, request) {
  return capture.stream(() => client.responses.create({ ...request, stream: true }), { input: request });
}
