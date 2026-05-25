const jwt = require("jsonwebtoken");
const { SECRET } = require("../utils/secrets");
const { readDb } = require("../utils/db");

module.exports = async (req, res, next) => {
  const authHeader = req.headers.authorization || "";
  const token = authHeader.startsWith("Bearer ")
    ? authHeader.slice(7).trim()
    : authHeader.trim();

  if (token) {
    try {
      const decoded = jwt.verify(token, SECRET);
      const db = await readDb();
      if (db.users.some((u) => u.id === decoded.id)) {
        req.user = decoded;
      }
    } catch {
      /* ignore invalid token, treat as anonymous */
    }
  }
  next();
};
