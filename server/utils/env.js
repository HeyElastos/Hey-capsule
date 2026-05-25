require("dotenv").config();
const path = require("path");
const { z } = require("zod");

// Anchor default data/uploads paths at the server directory so the resolved
// path doesn't depend on the cwd the process is launched from. In production
// (YunoHost, Docker, etc.) these should be overridden to a persistent volume
// outside the install dir — otherwise upgrades that re-fetch source wipe data.
const SERVER_ROOT = path.resolve(__dirname, "..");

const schema = z.object({
  NODE_ENV: z.enum(["development", "production", "test"]).default("development"),
  PORT: z.coerce.number().int().positive().default(4000),
  CLIENT_ORIGIN: z.string().optional(),
  RP_ID: z.string().default("localhost"),
  RP_NAME: z.string().default("Hey"),
  WEBAUTHN_ORIGIN: z.string().url().default("http://localhost:3000"),
  SECRET: z.string().min(16).optional(),
  REFRESH_SECRET: z.string().min(16).optional(),
  LOG_LEVEL: z.enum(["fatal", "error", "warn", "info", "debug", "trace"]).default("info"),
  DATA_DIR: z.string().default(path.join(SERVER_ROOT, "data")),
  UPLOADS_DIR: z.string().default(path.join(SERVER_ROOT, "uploads")),
});

const parsed = schema.safeParse(process.env);

if (!parsed.success) {
  const issues = parsed.error.issues
    .map((i) => `  - ${i.path.join(".")}: ${i.message}`)
    .join("\n");
  // Use stderr directly; pino isn't loaded yet at this point.
  process.stderr.write(`Invalid environment configuration:\n${issues}\n`);
  process.exit(1);
}

const env = parsed.data;

// Defaulting CLIENT_ORIGIN in dev mirrors the prior fallback in app.js.
if (!env.CLIENT_ORIGIN) {
  if (env.NODE_ENV === "production") {
    process.stderr.write("CLIENT_ORIGIN is required in production\n");
    process.exit(1);
  }
  env.CLIENT_ORIGIN = "http://localhost:3000";
}

env.isProd = env.NODE_ENV === "production";

// Resolve user-supplied paths to absolute form so downstream code can join
// against them safely (e.g. resolveUploadPath traversal checks).
env.DATA_DIR = path.resolve(env.DATA_DIR);
env.UPLOADS_DIR = path.resolve(env.UPLOADS_DIR);

module.exports = env;
