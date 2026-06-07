import type { EventPayload } from './shared';
import { Emitter } from './emitter';

export class Handler {
  constructor(private emitter: Emitter) {}

  handle(payload: EventPayload): void {
    this.emitter.forward(payload);
  }
}
