// The only place `invoke` is called (spec §17.3). Everything here is a
// thin re-export of the generated bindings, so a mistyped command name or
// a mismatched argument is a compile error, not a runtime one.

export { commands } from "../types/bindings";
