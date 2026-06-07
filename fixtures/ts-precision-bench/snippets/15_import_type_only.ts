import type { User, Session } from './types';
import type { Config } from './config';

export function greet(user: User): string {
  return `Hello ${user.name}`;
}
