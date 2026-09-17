type Handler = (args: Record<string, unknown> | undefined) => unknown;
type Listener = (event: { payload: unknown }) => void;

/** A stand-in for the Tauri runtime in tests: scripted commands and emitted events. */
class TauriMock {
  private handlers = new Map<string, Handler>();
  private listeners = new Map<string, Listener[]>();
  calls: { command: string; args: Record<string, unknown> | undefined }[] = [];
  opened: string[] = [];
  revealed: string[] = [];
  nextFolder: string | null = null;

  reset(): void {
    this.handlers.clear();
    this.listeners.clear();
    this.calls = [];
    this.opened = [];
    this.revealed = [];
    this.nextFolder = null;
  }

  /** Scripts a command's result; a thrown value becomes the rejection. */
  handle(command: string, handler: Handler): void {
    this.handlers.set(command, handler);
  }

  invoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
    this.calls.push({ command, args });
    const handler = this.handlers.get(command);
    if (!handler) {
      return Promise.resolve(null);
    }
    try {
      return Promise.resolve(handler(args));
    } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /** Rejects a command with a `{ code, message, fix }` value, as the shell does. */
  reject(command: string, view: { code: string; message: string; fix: string | null }): void {
    this.handlers.set(command, () => Promise.reject(Object.assign(new Error(view.message), view)));
  }

  listen(event: string, listener: Listener): Promise<() => void> {
    this.listeners.set(event, [...(this.listeners.get(event) ?? []), listener]);
    return Promise.resolve(() => {
      this.listeners.set(
        event,
        (this.listeners.get(event) ?? []).filter((l) => l !== listener),
      );
    });
  }

  emit(event: string, payload: unknown): void {
    for (const listener of this.listeners.get(event) ?? []) {
      listener({ payload });
    }
  }

  dialogOpen(): Promise<string | null> {
    return Promise.resolve(this.nextFolder);
  }

  openUrl(url: string): Promise<void> {
    this.opened.push(url);
    return Promise.resolve();
  }

  reveal(path: string): Promise<void> {
    this.revealed.push(path);
    return Promise.resolve();
  }
}

export const tauriMock = new TauriMock();
