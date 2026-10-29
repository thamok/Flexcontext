import { writeRecord } from './store';
export class Session {
  issue(token: string, expires: number) {
    return writeRecord(token, expires);
  }
  describe(token: string) {
    return `Session ${token}`;
  }
}
