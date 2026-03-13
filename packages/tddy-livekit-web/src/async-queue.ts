/**
 * AsyncQueue - backpressure-aware async channel for streaming responses.
 * Enables ConnectRPC StreamResponse.message (AsyncIterable) to work with
 * push-based LiveKit DataReceived events.
 */
export class AsyncQueue<T> {
  private queue: T[] = [];
  private waiting: Array<(value: T | null) => void> = [];
  private waitingReject: Array<(error: Error) => void> = [];
  private closed = false;
  private error: Error | null = null;

  enqueue(item: T): void {
    if (this.closed) return;

    if (this.waiting.length > 0) {
      const resolve = this.waiting.shift()!;
      this.waitingReject.shift();
      resolve(item);
    } else {
      this.queue.push(item);
    }
  }

  fail(error: Error): void {
    if (this.closed) return;
    this.closed = true;
    this.error = error;

    while (this.waitingReject.length > 0) {
      const reject = this.waitingReject.shift()!;
      this.waiting.shift();
      reject(error);
    }
  }

  async dequeue(): Promise<T | null> {
    if (this.queue.length > 0) {
      return this.queue.shift()!;
    }

    if (this.closed && this.error) {
      throw this.error;
    }

    if (this.closed) {
      return null;
    }

    return new Promise<T | null>((resolve, reject) => {
      this.waiting.push(resolve);
      this.waitingReject.push(reject);
    });
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;

    while (this.waiting.length > 0) {
      const resolve = this.waiting.shift()!;
      this.waitingReject.shift();
      resolve(null);
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterableIterator<T> {
    while (true) {
      const item = await this.dequeue();
      if (item === null) break;
      yield item;
    }
  }
}
