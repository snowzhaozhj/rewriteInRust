export interface Config {
  host: string;
  port: number;
  debug: boolean;
}

export type Handler<T> = (req: T) => Promise<void>;

export enum LogLevel {
  Debug = 0,
  Info = 1,
  Warn = 2,
  Error = 3,
}
