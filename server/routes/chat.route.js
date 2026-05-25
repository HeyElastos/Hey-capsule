const express = require("express");
const multer = require("multer");
const auth = require("../middlewares/auth");
const { ALLOWED_MIMES } = require("../utils/media");
const {
  listThreads,
  getThread,
  sendMessage,
  editMessage,
  deleteMessage,
  reactToMessage,
  markThreadRead,
  followPeer,
  uploadAttachments,
  createRoom,
  listRooms,
  getRoom,
  sendRoomMessage,
  addRoomMember,
  removeRoomMember,
} = require("../controllers/chat.controller");

// Same constraints as post uploads: 25MB per file, up to 4 per message.
const upload = multer({
  storage: multer.memoryStorage(),
  limits: { fileSize: 25 * 1024 * 1024, files: 4 },
  fileFilter: (_req, file, cb) => {
    if (ALLOWED_MIMES.has(file.mimetype)) cb(null, true);
    else cb(new Error("Unsupported file type"));
  },
});

const router = express.Router();

router.get("/threads", auth, listThreads);
router.get("/threads/:peerDid", auth, getThread);
router.post("/threads/:peerDid/messages", auth, sendMessage);
router.post("/threads/:peerDid/read", auth, markThreadRead);
router.post("/attachments", auth, upload.array("media", 4), uploadAttachments);
router.patch("/messages/:id", auth, editMessage);
router.delete("/messages/:id", auth, deleteMessage);
router.post("/messages/:id/reactions", auth, reactToMessage);
router.post("/follow", auth, followPeer);

// Group chat ("room") routes.
router.get("/rooms", auth, listRooms);
router.post("/rooms", auth, createRoom);
router.get("/rooms/:id", auth, getRoom);
router.post("/rooms/:id/messages", auth, sendRoomMessage);
router.post("/rooms/:id/members", auth, addRoomMember);
router.delete("/rooms/:id/members/:did", auth, removeRoomMember);

module.exports = router;
