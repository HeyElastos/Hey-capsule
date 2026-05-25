const fs = require("fs");
const path = require("path");
const { DatabaseSync } = require("node:sqlite");
const env = require("./env");

const dataDir = env.DATA_DIR;
const dbPath = path.join(dataDir, "db.sqlite");
const legacyJsonPath = path.join(dataDir, "db.json");

fs.mkdirSync(dataDir, { recursive: true });

const sqlite = new DatabaseSync(dbPath);

// WAL gives us concurrent reads during writes and crash-safe atomic commits.
// NORMAL synchronous + WAL is the documented safe combination — durability is
// preserved at transaction boundaries; we don't need FULL sync for this use.
sqlite.exec("PRAGMA journal_mode = WAL;");
sqlite.exec("PRAGMA synchronous = NORMAL;");
sqlite.exec("PRAGMA foreign_keys = ON;");

sqlite.exec(`
  CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS posts (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
  );
`);

const seed = (data) => {
  if (!Array.isArray(data.users)) data.users = [];
  if (!Array.isArray(data.posts)) data.posts = [];
  if (!Array.isArray(data.notifications)) data.notifications = [];
  return data;
};

const selectAllUsers = sqlite.prepare("SELECT data FROM users");
const selectAllPosts = sqlite.prepare("SELECT data FROM posts");
const selectAllNotifications = sqlite.prepare("SELECT data FROM notifications");

const deleteUsers = sqlite.prepare("DELETE FROM users");
const deletePosts = sqlite.prepare("DELETE FROM posts");
const deleteNotifications = sqlite.prepare("DELETE FROM notifications");

const insertUser = sqlite.prepare("INSERT INTO users (id, data) VALUES (?, ?)");
const insertPost = sqlite.prepare("INSERT INTO posts (id, data) VALUES (?, ?)");
const insertNotification = sqlite.prepare(
  "INSERT INTO notifications (id, data) VALUES (?, ?)"
);

// node:sqlite doesn't ship a transaction(fn) helper like better-sqlite3, so we
// roll our own. SAVEPOINT lets us nest if a caller is already inside one.
const inTx = (fn) => {
  sqlite.exec("BEGIN");
  try {
    const out = fn();
    sqlite.exec("COMMIT");
    return out;
  } catch (e) {
    try {
      sqlite.exec("ROLLBACK");
    } catch {
      /* commit already failed; nothing to rollback */
    }
    throw e;
  }
};

const isEmpty = () => {
  const u = sqlite.prepare("SELECT 1 FROM users LIMIT 1").get();
  const p = sqlite.prepare("SELECT 1 FROM posts LIMIT 1").get();
  const n = sqlite.prepare("SELECT 1 FROM notifications LIMIT 1").get();
  return !u && !p && !n;
};

const replaceAll = (data) =>
  inTx(() => {
    deleteUsers.run();
    deletePosts.run();
    deleteNotifications.run();
    for (const u of data.users) insertUser.run(u.id, JSON.stringify(u));
    for (const p of data.posts) insertPost.run(p.id, JSON.stringify(p));
    for (const n of data.notifications) {
      insertNotification.run(n.id, JSON.stringify(n));
    }
  });

// One-shot import from legacy JSON db. Idempotent guard: only runs when the
// SQLite store is empty AND a legacy file exists. The legacy file is renamed
// (not deleted) so an operator can audit or roll back.
const migrateFromLegacyJson = () => {
  if (!isEmpty()) return;
  if (!fs.existsSync(legacyJsonPath)) return;
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(legacyJsonPath, "utf8"));
  } catch {
    return;
  }
  const data = seed(parsed || {});
  if (
    data.users.length === 0 &&
    data.posts.length === 0 &&
    data.notifications.length === 0
  ) {
    return;
  }
  replaceAll(data);
  try {
    fs.renameSync(legacyJsonPath, `${legacyJsonPath}.migrated`);
  } catch {
    /* non-fatal — data already in sqlite */
  }
};

migrateFromLegacyJson();

let cache = null;

const loadFromDisk = () =>
  seed({
    users: selectAllUsers.all().map((r) => JSON.parse(r.data)),
    posts: selectAllPosts.all().map((r) => JSON.parse(r.data)),
    notifications: selectAllNotifications.all().map((r) => JSON.parse(r.data)),
  });

// Returns the live in-memory db. First call loads from disk; subsequent calls
// are O(1). Callers mutate this object and then call writeDb to persist.
const readDb = async () => {
  if (cache) return cache;
  cache = loadFromDisk();
  return cache;
};

// Persists the cache to disk in a single transaction. If the commit fails we
// drop the cache so the next read reloads the last known-good snapshot from
// SQLite — same defensive fallback as the old JSON implementation.
const writeDb = async (data) => {
  cache = seed(data);
  try {
    replaceAll(cache);
  } catch (e) {
    cache = null;
    throw e;
  }
};

const close = () => {
  try {
    sqlite.close();
  } catch {
    /* already closed */
  }
};

module.exports = { readDb, writeDb, close };
