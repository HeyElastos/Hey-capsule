const crypto = require("crypto");
const fs = require("fs");
const path = require("path");
const env = require("./env");

const SECRETS_FILE = path.join(__dirname, "../data/.secrets.json");

const loadOrCreateDevSecrets = () => {
  try {
    const raw = fs.readFileSync(SECRETS_FILE, "utf8");
    const parsed = JSON.parse(raw);
    if (parsed.SECRET && parsed.REFRESH_SECRET) return parsed;
  } catch {
    /* file missing or unreadable — fall through to regenerate */
  }
  const generated = {
    SECRET: crypto.randomBytes(48).toString("hex"),
    REFRESH_SECRET: crypto.randomBytes(48).toString("hex"),
  };
  try {
    fs.mkdirSync(path.dirname(SECRETS_FILE), { recursive: true });
    fs.writeFileSync(SECRETS_FILE, JSON.stringify(generated, null, 2), {
      mode: 0o600,
    });
    console.warn(
      `[secrets] generated dev secrets at ${SECRETS_FILE}; delete this file to rotate.`
    );
  } catch (e) {
    console.warn(
      `[secrets] could not persist dev secrets (${e.message}); using ephemeral values for this process.`
    );
  }
  return generated;
};

const resolve = (name, dev) => {
  const v = env[name];
  if (v && v.length >= 16) return v;
  if (env.isProd) {
    throw new Error(
      `Refusing to start: env ${name} is required (>= 16 chars) in production.`
    );
  }
  return dev[name];
};

const devSecrets = env.isProd ? null : loadOrCreateDevSecrets();

module.exports = {
  SECRET: resolve("SECRET", devSecrets || {}),
  REFRESH_SECRET: resolve("REFRESH_SECRET", devSecrets || {}),
};
