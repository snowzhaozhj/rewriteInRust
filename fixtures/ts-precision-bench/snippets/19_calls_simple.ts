import { process, validate, transform } from './utils';

const raw = validate(input);
const data = process(raw);
const result = transform(data, { strict: true });
