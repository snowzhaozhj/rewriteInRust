import type { EventName, EventPayload } from './shared';
import { Handler } from './handler';

export class EventBus {
  private handlers: Map<EventName, Handler[]> = new Map();

  register(event: EventName, handler: Handler): void {
    const list = this.handlers.get(event) ?? [];
    list.push(handler);
    this.handlers.set(event, list);
  }

  emit(event: EventName, payload: EventPayload): void {
    const list = this.handlers.get(event) ?? [];
    list.forEach(h => h.handle(payload));
  }
}
