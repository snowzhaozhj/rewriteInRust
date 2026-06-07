export interface User {
  id: string;
  name: string;
  email: string;
}

export interface Session {
  token: string;
  user: User;
  expiresAt: Date;
}

export enum Role { Admin = 'admin', User = 'user', Guest = 'guest' }

export interface Serializable {
  serialize(): string;
}
