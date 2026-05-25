const jwt = require("jsonwebtoken");
const { SECRET } = require("../utils/secrets");
const { readDb } = require("../utils/db");

module.exports = async (req, res, next) => {
  const authHeader = req.headers.authorization || "";
  const token = authHeader.startsWith("Bearer ")
    ? authHeader.slice(7).trim()
    : authHeader.trim();

  if (!token) {
    return res.status(401).json({ message: "Missing authorization token" });
  }

  try {
    const decoded = jwt.verify(token, SECRET);
    // Verify the user still exists. Cheap because db is in-memory cached.
    const db = await readDb();
    const exists = db.users.some((u) => u.id === decoded.id);
    if (!exists) {
      return res.status(401).json({ message: "Invalid or expired token" });
    }
    req.user = decoded;
    next();
  } catch {
    return res.status(401).json({ message: "Invalid or expired token" });
  }
};
