const fs = require("fs/promises");
const path = require("path");
const dbPath = path.join(__dirname, "../data/db.json");

const readDb = async () => {
  try {
    const file = await fs.readFile(dbPath, "utf8");
    const data = JSON.parse(file);
    if (!Array.isArray(data.users)) data.users = [];
    if (!Array.isArray(data.posts)) data.posts = [];
    return data;
  } catch (error) {
    const initial = { users: [], posts: [] };
    await writeDb(initial);
    return initial;
  }
};

const writeDb = async (data) => {
  await fs.mkdir(path.dirname(dbPath), { recursive: true });
  await fs.writeFile(dbPath, JSON.stringify(data, null, 2), "utf8");
};

module.exports = { readDb, writeDb };
