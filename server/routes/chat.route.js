const express = require("express");
const auth = require("../middlewares/auth");
const {
  listThreads,
  getThread,
  sendMessage,
  followPeer,
} = require("../controllers/chat.controller");

const router = express.Router();

router.get("/threads", auth, listThreads);
router.get("/threads/:peerDid", auth, getThread);
router.post("/threads/:peerDid/messages", auth, sendMessage);
router.post("/follow", auth, followPeer);

module.exports = router;
