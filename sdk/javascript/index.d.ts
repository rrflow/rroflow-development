export type Agent = 'codex' | 'claude' | 'grok';
export interface Scope { input?: unknown; operationId?: string; parentOperationId?: string | null }
export interface CaptureOptions { agent: Agent; sessionId: string; devspace: string; spool: string; endpoint: string; executable?: string; content?: boolean }
export class Capture {
  constructor(options: CaptureOptions);
  readonly agent: Agent;
  readonly sessionId: string;
  readonly captureId: string;
  record(kind: string, options?: { operationId?: string; parentOperationId?: string | null; nativeType?: string | null; payload?: unknown }): Promise<{ event_id: string; durable: true; delivery: 'queued' }>;
  stream<T>(create: () => AsyncIterable<T> | Promise<AsyncIterable<T>>, scope?: Scope): AsyncGenerator<T>;
  call<T>(run: () => Promise<T> | T, scope?: Scope): Promise<T>;
  tool<T>(name: string, run: () => Promise<T> | T, scope?: Scope): Promise<T>;
  flush(): Promise<{ acknowledged: number }>;
  close(options?: { flush?: boolean }): Promise<{ acknowledged: number } | void>;
}
export function codexStream<T>(capture: Capture, thread: { runStreamed(input: any, options?: any): Promise<{ events: AsyncIterable<T> }> }, input: any, options?: any): AsyncGenerator<T>;
export function claudeStream<T>(capture: Capture, query: (args: any) => AsyncIterable<T>, args: { prompt: any; options?: any }): AsyncGenerator<T>;
export function grokStream<T>(capture: Capture, client: { responses: { create(request: any): AsyncIterable<T> | Promise<AsyncIterable<T>> } }, request: any): AsyncGenerator<T>;
