import { AuthService } from './auth';
import { findUser } from './db';
import type { User } from './types';

export async function login(token: string): Promise<User | null> {
  const service = new AuthService();
  const session = await service.authenticate(token);
  return session?.user ?? null;
}

export { AuthService } from './auth';
