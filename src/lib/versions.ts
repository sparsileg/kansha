// The app and schema versions, shown on the start screen and in About
// Kansha; "unknown" when either cannot be read.
import { commands } from "./api";

export interface Versions {
  app: string;
  schema: string;
}

export async function loadVersions(): Promise<Versions> {
  const [app, schema] = await Promise.all([
    commands.appVersion().catch(() => "unknown"),
    commands
      .schemaVersion()
      .then(String)
      .catch(() => "unknown"),
  ]);
  return { app, schema };
}
