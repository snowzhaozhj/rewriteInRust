import { EventEmitter } from 'events';
import { Logger } from './logger';

const emitter = new EventEmitter();
const logger = new Logger();
emitter.on("data", (d: string) => logger.info(d));
emitter.emit("data", "hello");
console.log("done");
