export interface Config {
  host: string;
  port: number;
}

export type Handler = (req: Request) => Response;

export enum LogLevel {
  Debug,
  Info,
  Warn,
  Error,
}
