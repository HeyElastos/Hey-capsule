const router = require("express").Router();
const { list, markAllRead, remove } = require("../controllers/notification.controller");
const requireAuth = require("../middlewares/auth");

router.get("/", requireAuth, list);
router.post("/read-all", requireAuth, markAllRead);
router.delete("/:id", requireAuth, remove);

module.exports = router;
