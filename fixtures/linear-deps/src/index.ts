import { NumberService } from './service';
export { clamp, Range } from './utils';

const svc = new NumberService({ min: 0, max: 100 });
export default svc;
