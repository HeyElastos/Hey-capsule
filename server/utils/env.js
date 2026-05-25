require("dotenv").config();
const { z } = require("zod");

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

module.exports = env;
