// A stand-in for `registerState` in shell tests: enough to open an account.
export function registerStub(open: (id: number) => void) {
  return {
    accountId: null as number | null,
    async open(id: number) {
      open(id);
      this.accountId = id;
    },
  };
}
