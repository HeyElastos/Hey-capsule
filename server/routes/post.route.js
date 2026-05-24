const express = require("express");
const multer = require("multer");
const {
  createPost,
  getPosts,
  getPost,
  getUserPosts,
  reactToPost,
  repostPost,
  addComment,
  deleteComment,
  deletePost,
} = require("../controllers/post.controller");
const requireAuth = require("../middlewares/auth");
const optionalAuth = require("../middlewares/optionalAuth");

const upload = multer({
  storage: multer.memoryStorage(),
  limits: { fileSize: 100 * 1024 * 1024, files: 12 },
});

const router = express.Router();

router.get("/", optionalAuth, getPosts);
router.get("/by-user/:id", optionalAuth, getUserPosts);
router.get("/:id", optionalAuth, getPost);
router.post("/", requireAuth, upload.array("media", 12), createPost);
router.delete("/:id", requireAuth, deletePost);

router.post("/:id/react", requireAuth, reactToPost);
router.post("/:id/repost", requireAuth, repostPost);
router.post("/:id/comments", requireAuth, addComment);
router.delete("/:id/comments/:commentId", requireAuth, deleteComment);

module.exports = router;
