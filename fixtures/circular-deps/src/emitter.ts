import type { EventPayload } from './shared';
import { EventBus } from './event-bus';

export class Emitter {
  constructor(private bus: EventBus) {}

  forward(payload: EventPayload): void {
    this.bus.emit('forwarded', payload);
  }
}
