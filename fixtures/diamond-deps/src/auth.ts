import type { User, Session, Serializable } from './types';
import { findUser, generateId } from './db';

export class AuthService implements Serializable {
  private sessions: Map<string, Session> = new Map();

  async authenticate(token: string): Promise<Session | null> {
    const user = await findUser(token);
    if (!user) return null;
    const session: Session = { token: generateId(), user, expiresAt: new Date() };
    this.sessions.set(session.token, session);
    return session;
  }

  serialize(): string {
    return JSON.stringify([...this.sessions.entries()]);
  }
}
