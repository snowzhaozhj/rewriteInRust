import type { User } from './types';

export async function findUser(id: string): Promise<User | null> {
  return null;
}

export async function saveUser(user: User): Promise<void> {}

export function generateId(): string {
  return Math.random().toString(36).slice(2);
}
