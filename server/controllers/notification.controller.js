const { readDb, writeDb } = require("../utils/db");

const list = async (req, res) => {
  try {
    const db = await readDb();
    const notifications = (db.notifications || [])
      .filter((n) => n.userId === req.user.id)
      .sort((a, b) => new Date(b.createdAt) - new Date(a.createdAt));
    res.status(200).json({ notifications });
  } catch (error) {
    res.status(500).json({ message: "Unable to load notifications", error: error.message });
  }
};

const markAllRead = async (req, res) => {
  try {
    const db = await readDb();
    let changed = false;
    for (const n of db.notifications || []) {
      if (n.userId === req.user.id && !n.read) {
        n.read = true;
        changed = true;
      }
    }
    if (changed) await writeDb(db);
    res.status(200).json({ message: "ok" });
  } catch (error) {
    res.status(500).json({ message: "Unable to update", error: error.message });
  }
};

const remove = async (req, res) => {
  try {
    const db = await readDb();
    const before = (db.notifications || []).length;
    db.notifications = (db.notifications || []).filter(
      (n) => !(n.id === req.params.id && n.userId === req.user.id)
    );
    if (db.notifications.length < before) await writeDb(db);
    res.status(200).json({ message: "ok" });
  } catch (error) {
    res.status(500).json({ message: "Unable to delete", error: error.message });
  }
};

module.exports = { list, markAllRead, remove };
