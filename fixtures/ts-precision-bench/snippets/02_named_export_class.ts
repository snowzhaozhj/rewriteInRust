export class Logger {
  log(msg: string): void {
    console.log(msg);
  }
}

export class Formatter {
  format(s: string): string {
    return s.trim();
  }
}
