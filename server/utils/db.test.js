import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { describe, it, expect, beforeEach, afterEach } from "vitest";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

let tmpDir;
let prevBackup;

const dataDir = path.join(__dirname, "../data");

const loadFreshDb = () => {
  const dbPath = require.resolve("./db.js");
  delete require.cache[dbPath];
  return require("./db.js");
};

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "hey-db-test-"));
  prevBackup = null;
  if (fs.existsSync(dataDir)) {
    prevBackup = `${dataDir}.testbackup-${Date.now()}`;
    fs.renameSync(dataDir, prevBackup);
  }
  fs.symlinkSync(tmpDir, dataDir, "dir");
});

afterEach(() => {
  try {
    const { close } = require("./db.js");
    close();
  } catch {
    /* db.js may not have been loaded */
  }
  delete require.cache[require.resolve("./db.js")];
  try {
    fs.unlinkSync(dataDir);
  } catch {
    /* not a symlink */
  }
  if (prevBackup && fs.existsSync(prevBackup)) {
    fs.renameSync(prevBackup, dataDir);
  }
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

describe("db.js — readDb/writeDb roundtrip", () => {
  it("returns a seeded empty db on first read", async () => {
    const { readDb } = loadFreshDb();
    const db = await readDb();
    expect(db).toEqual({ users: [], posts: [], notifications: [], chatMessages: [] });
  });

  it("persists writes across cache invalidation", async () => {
    const { readDb, writeDb, close } = loadFreshDb();
    const db = await readDb();
    db.users.push({ id: "u1", name: "alice" });
    db.posts.push({ id: "p1", userId: "u1", caption: "hi" });
    db.notifications.push({
      id: "n1",
      userId: "u1",
      type: "comment",
      createdAt: new Date().toISOString(),
    });
    await writeDb(db);

    // Drop the SQLite handle and reopen to confirm durability.
    close();
    delete require.cache[require.resolve("./db.js")];
    const fresh = require("./db.js");
    const reread = await fresh.readDb();
    expect(reread.users).toHaveLength(1);
    expect(reread.users[0]).toEqual({ id: "u1", name: "alice" });
    expect(reread.posts[0].caption).toBe("hi");
    expect(reread.notifications[0].type).toBe("comment");
  });

  it("replaces rows on overwrite (delete + insert semantics)", async () => {
    const { readDb, writeDb } = loadFreshDb();
    const db = await readDb();
    db.users.push({ id: "u1", name: "alice" });
    db.users.push({ id: "u2", name: "bob" });
    await writeDb(db);

    db.users = db.users.filter((u) => u.id !== "u1");
    await writeDb(db);

    const after = await readDb();
    expect(after.users).toHaveLength(1);
    expect(after.users[0].id).toBe("u2");
  });

  it("migrates from legacy db.json on first boot", async () => {
    fs.writeFileSync(
      path.join(dataDir, "db.json"),
      JSON.stringify({
        users: [{ id: "legacy-u", name: "migrated" }],
        posts: [],
        notifications: [],
      })
    );
    const { readDb } = loadFreshDb();
    const db = await readDb();
    expect(db.users).toHaveLength(1);
    expect(db.users[0].name).toBe("migrated");
    expect(fs.existsSync(path.join(dataDir, "db.json"))).toBe(false);
    expect(fs.existsSync(path.join(dataDir, "db.json.migrated"))).toBe(true);
  });
});
