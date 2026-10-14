// Test-only identity injection. The production CLI has no auth bypass flag or environment variable.
import { AccessService, type AccessStatus } from '../../src/modules/access/service.js';
export class TestAccess extends AccessService {
  state: AccessStatus['state'] = 'signed_in';
  constructor(public user = { id: 'test-user', email: 'test@example.com' }) {
    super({
      async load() {
        return undefined;
      },
      async save() {},
      async clear() {},
    });
  }
  override async status(): Promise<AccessStatus> {
    return {
      state: this.state,
      user: this.state === 'signed_in' ? this.user : null,
    };
  }
  override async logout() {
    this.state = 'signed_out';
    return this.status();
  }
}
