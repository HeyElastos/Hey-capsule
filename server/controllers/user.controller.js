const crypto = require("crypto");
const jwt = require("jsonwebtoken");
const path = require("path");
const fs = require("fs/promises");
const sharp = require("sharp");
const { readDb, writeDb } = require("../utils/db");
const { createNotification, removeFollowRequestNotification } = require("../utils/notifications");
const logger = require("../utils/logger");

const { SECRET, REFRESH_SECRET } = require("../utils/secrets");
const env = require("../utils/env");
const UPLOADS_DIR = env.UPLOADS_DIR;
const AVATARS_DIR = path.join(UPLOADS_DIR, "avatars");

// Resolve `/uploads/...` URL → absolute filesystem path, refusing any path
// that escapes the uploads dir (no '..' traversal). Returns null for bad input.
const resolveUploadPath = (urlPath) => {
  if (typeof urlPath !== "string") return null;
  if (!urlPath.startsWith("/uploads/")) return null;
  const abs = path.resolve(path.join(UPLOADS_DIR, urlPath.slice("/uploads/".length)));
  if (!abs.startsWith(UPLOADS_DIR + path.sep)) return null;
  return abs;
};

const safeUnlink = async (urlPath, label) => {
  const abs = resolveUploadPath(urlPath);
  if (!abs) return false;
  try {
    await fs.unlink(abs);
    return true;
  } catch (e) {
    if (e.code !== "ENOENT") {
      logger.warn({ err: e, file: abs, label }, "failed to delete file during account deletion");
    }
    return false;
  }
};

const ensureSocial = (user) => {
  if (!Array.isArray(user.followers)) user.followers = [];
  if (!Array.isArray(user.following)) user.following = [];
  if (!Array.isArray(user.pendingFollowers)) user.pendingFollowers = [];
  if (!Array.isArray(user.pendingFollowing)) user.pendingFollowing = [];
};

const publicUser = (user) => {
  ensureSocial(user);
  return {
    id: user.id,
    name: user.name,
    bio: user.bio || "",
    avatar: user.avatar || "",
    role: user.role,
    counts: {
      followers: user.followers.length,
      following: user.following.length,
    },
  };
};

const relationship = (viewer, target) => {
  if (!viewer || viewer.id === target.id) return "self";
  ensureSocial(viewer);
  ensureSocial(target);
  if (target.followers.includes(viewer.id)) return "following";
  if (target.pendingFollowers.includes(viewer.id)) return "requested";
  if (viewer.pendingFollowers.includes(target.id)) return "incoming";
  return "none";
};

const hashKey = (key) =>
  crypto.createHash("sha256").update(key || "").digest("hex");

const signTokens = (user) => {
  const payload = { id: user.id, name: user.name };
  const accessToken = jwt.sign(payload, SECRET, { expiresIn: "6h" });
  const refreshToken = jwt.sign(payload, REFRESH_SECRET, { expiresIn: "7d" });
  return { accessToken, refreshToken };
};

const refresh = async (req, res) => {
  try {
    const { refreshToken } = req.body || {};
    if (!refreshToken || typeof refreshToken !== "string") {
      return res.status(400).json({ message: "Refresh token required" });
    }
    let decoded;
    try {
      decoded = jwt.verify(refreshToken, REFRESH_SECRET);
    } catch {
      return res.status(401).json({ message: "Invalid or expired refresh token" });
    }
    const db = await readDb();
    const user = db.users.find((u) => u.id === decoded.id);
    if (!user) return res.status(401).json({ message: "User no longer exists" });
    const tokens = signTokens(user);
    return res.status(200).json({
      user: publicUser(user),
      ...tokens,
      accessTokenUpdatedAt: new Date().toISOString(),
    });
  } catch (error) {
    return res.status(500).json({ message: "Could not refresh token" });
  }
};

const signup = async (req, res) => {
  try {
    const { name } = req.body;

    if (!name || typeof name !== "string" || !name.trim()) {
      return res.status(400).json({ message: "Display name is required" });
    }

    const displayName = name.trim().slice(0, 30);
    const authKey = crypto.randomBytes(32).toString("hex");
    const authKeyHash = hashKey(authKey);

    const db = await readDb();
    if (db.users.some((u) => (u.name || "").toLowerCase() === displayName.toLowerCase())) {
      return res.status(409).json({ message: "Name already in use" });
    }
    const user = {
      id: crypto.randomUUID(),
      name: displayName,
      authKeyHash,
      role: "general",
      avatar: "",
      bio: "",
      followers: [],
      following: [],
      pendingFollowers: [],
      pendingFollowing: [],
      createdAt: new Date().toISOString(),
    };

    db.users.push(user);
    await writeDb(db);

    const tokens = signTokens(user);

    return res.status(201).json({
      message: "User created successfully",
      user: publicUser(user),
      authKey,
      ...tokens,
      accessTokenUpdatedAt: new Date().toISOString(),
    });
  } catch (error) {
    return res.status(500).json({ message: "Signup failed" });
  }
};

const signin = async (req, res) => {
  try {
    const { authKey } = req.body;

    if (!authKey || typeof authKey !== "string" || !authKey.trim()) {
      return res.status(400).json({ message: "Hey key is required" });
    }

    const authKeyHash = hashKey(authKey.trim());
    const db = await readDb();
    const user = db.users.find((item) => item.authKeyHash === authKeyHash);

    if (!user) {
      return res.status(401).json({ message: "Invalid Hey key" });
    }

    const tokens = signTokens(user);
    return res.status(200).json({
      message: "Signed in successfully",
      user: publicUser(user),
      ...tokens,
      accessTokenUpdatedAt: new Date().toISOString(),
    });
  } catch (error) {
    return res.status(500).json({ message: "Signin failed" });
  }
};

const me = async (req, res) => {
  try {
    const db = await readDb();
    const user = db.users.find((item) => item.id === req.user.id);
    if (!user) {
      return res.status(404).json({ message: "User not found" });
    }
    return res.status(200).json({
      ...publicUser(user),
      createdAt: user.createdAt,
    });
  } catch (error) {
    return res.status(500).json({ message: "Unable to load profile" });
  }
};

const getUserById = async (req, res) => {
  try {
    const db = await readDb();
    const user = db.users.find((item) => item.id === req.params.id);
    if (!user) {
      return res.status(404).json({ message: "User not found" });
    }
    const viewer = req.user
      ? db.users.find((u) => u.id === req.user.id)
      : null;
    return res.status(200).json({
      user: publicUser(user),
      relationship: relationship(viewer, user),
    });
  } catch (error) {
    return res.status(500).json({ message: "Unable to load user" });
  }
};

const requestFollow = async (req, res) => {
  try {
    if (req.params.id === req.user.id) {
      return res.status(400).json({ message: "Cannot follow yourself" });
    }
    const db = await readDb();
    const viewer = db.users.find((u) => u.id === req.user.id);
    const target = db.users.find((u) => u.id === req.params.id);
    if (!viewer || !target) {
      return res.status(404).json({ message: "User not found" });
    }
    ensureSocial(viewer);
    ensureSocial(target);

    if (target.followers.includes(viewer.id)) {
      return res.status(200).json({ relationship: "following" });
    }
    if (target.pendingFollowers.includes(viewer.id)) {
      return res.status(200).json({ relationship: "requested" });
    }
    target.pendingFollowers.push(viewer.id);
    viewer.pendingFollowing.push(target.id);

    createNotification(db, {
      userId: target.id,
      type: "follow_request",
      fromUserId: viewer.id,
      fromUserName: viewer.name,
      fromUserAvatar: viewer.avatar || "",
    });

    await writeDb(db);
    res.status(200).json({ relationship: "requested" });
  } catch (error) {
    res.status(500).json({ message: "Unable to follow" });
  }
};

const cancelFollowRequest = async (req, res) => {
  try {
    const db = await readDb();
    const viewer = db.users.find((u) => u.id === req.user.id);
    const target = db.users.find((u) => u.id === req.params.id);
    if (!viewer || !target) {
      return res.status(404).json({ message: "User not found" });
    }
    ensureSocial(viewer);
    ensureSocial(target);

    target.pendingFollowers = target.pendingFollowers.filter((id) => id !== viewer.id);
    viewer.pendingFollowing = viewer.pendingFollowing.filter((id) => id !== target.id);
    target.followers = target.followers.filter((id) => id !== viewer.id);
    viewer.following = viewer.following.filter((id) => id !== target.id);

    removeFollowRequestNotification(db, { userId: target.id, fromUserId: viewer.id });

    await writeDb(db);
    res.status(200).json({ relationship: "none" });
  } catch (error) {
    res.status(500).json({ message: "Unable to unfollow" });
  }
};

const acceptFollow = async (req, res) => {
  try {
    const db = await readDb();
    const me = db.users.find((u) => u.id === req.user.id);
    const requester = db.users.find((u) => u.id === req.params.id);
    if (!me || !requester) {
      return res.status(404).json({ message: "User not found" });
    }
    ensureSocial(me);
    ensureSocial(requester);

    if (!me.pendingFollowers.includes(requester.id)) {
      return res.status(400).json({ message: "No pending request" });
    }
    me.pendingFollowers = me.pendingFollowers.filter((id) => id !== requester.id);
    requester.pendingFollowing = requester.pendingFollowing.filter((id) => id !== me.id);
    if (!me.followers.includes(requester.id)) me.followers.push(requester.id);
    if (!requester.following.includes(me.id)) requester.following.push(me.id);

    removeFollowRequestNotification(db, { userId: me.id, fromUserId: requester.id });

    createNotification(db, {
      userId: requester.id,
      type: "follow_accepted",
      fromUserId: me.id,
      fromUserName: me.name,
      fromUserAvatar: me.avatar || "",
    });

    await writeDb(db);
    res.status(200).json({ relationship: "incoming-accepted" });
  } catch (error) {
    res.status(500).json({ message: "Unable to accept" });
  }
};

const rejectFollow = async (req, res) => {
  try {
    const db = await readDb();
    const me = db.users.find((u) => u.id === req.user.id);
    const requester = db.users.find((u) => u.id === req.params.id);
    if (!me || !requester) {
      return res.status(404).json({ message: "User not found" });
    }
    ensureSocial(me);
    ensureSocial(requester);

    me.pendingFollowers = me.pendingFollowers.filter((id) => id !== requester.id);
    requester.pendingFollowing = requester.pendingFollowing.filter((id) => id !== me.id);

    removeFollowRequestNotification(db, { userId: me.id, fromUserId: requester.id });

    await writeDb(db);
    res.status(200).json({ relationship: "none" });
  } catch (error) {
    res.status(500).json({ message: "Unable to reject" });
  }
};

const updateMe = async (req, res) => {
  try {
    const db = await readDb();
    const user = db.users.find((item) => item.id === req.user.id);
    if (!user) {
      return res.status(404).json({ message: "User not found" });
    }

    if (typeof req.body.name === "string") {
      const name = req.body.name.trim().slice(0, 30);
      if (name) user.name = name;
    }

    if (typeof req.body.bio === "string") {
      user.bio = req.body.bio.trim().slice(0, 280);
    }

    if (req.file) {
      await fs.mkdir(AVATARS_DIR, { recursive: true });
      const fileName = `${user.id}-${Date.now()}.avif`;
      const outputPath = path.join(AVATARS_DIR, fileName);

      await sharp(req.file.buffer)
        .rotate()
        .resize(512, 512, { fit: "cover" })
        .avif({ quality: 75 })
        .toFile(outputPath);

      if (user.avatar && user.avatar.startsWith("/uploads/avatars/")) {
        const oldPath = resolveUploadPath(user.avatar);
        if (oldPath) fs.unlink(oldPath).catch(() => {});
      }

      user.avatar = `/uploads/avatars/${fileName}`;
    }

    await writeDb(db);
    return res.status(200).json({ user: publicUser(user) });
  } catch (error) {
    return res.status(500).json({ message: "Could not update profile" });
  }
};

const deleteMe = async (req, res) => {
  try {
    const db = await readDb();
    const meId = req.user.id;
    const user = db.users.find((u) => u.id === meId);
    if (!user) return res.status(404).json({ message: "User not found" });

    let filesDeleted = 0;

    // Delete current avatar file (if any)
    if (await safeUnlink(user.avatar, "current avatar")) filesDeleted++;

    // Defensive: also delete any orphaned avatars matching `{userId}-*.avif` —
    // these can exist if a past updateMe replaced the avatar but the old file
    // wasn't successfully unlinked (we previously swallowed those errors).
    try {
      const entries = await fs.readdir(AVATARS_DIR);
      for (const name of entries) {
        if (name.startsWith(`${meId}-`)) {
          if (await safeUnlink(`/uploads/avatars/${name}`, "orphan avatar")) {
            filesDeleted++;
          }
        }
      }
    } catch (e) {
      if (e.code !== "ENOENT") {
        logger.warn({ err: e }, "could not scan avatars dir during deletion");
      }
    }

    // Delete all of the user's posts (and their uploaded media files)
    const myPosts = db.posts.filter((p) => p.userId === meId);
    let postsDeleted = myPosts.length;
    for (const post of myPosts) {
      for (const img of post.images || []) {
        if (await safeUnlink(img.url, `post media (${post.id})`)) filesDeleted++;
      }
    }
    db.posts = db.posts.filter((p) => p.userId !== meId);

    // Strip me from other users' posts (reactions, reposts, comments).
    // Comments: also cascade-delete any replies whose parent was mine so the
    // tree doesn't end up with dangling parentIds pointing at vanished nodes.
    for (const post of db.posts) {
      if (post.reactions) {
        for (const emoji of Object.keys(post.reactions)) {
          post.reactions[emoji] = (post.reactions[emoji] || []).filter(
            (id) => id !== meId
          );
          if (post.reactions[emoji].length === 0) delete post.reactions[emoji];
        }
      }
      if (Array.isArray(post.reposts)) {
        post.reposts = post.reposts.filter((r) => r.userId !== meId);
      }
      if (Array.isArray(post.comments)) {
        const myCommentIds = new Set(
          post.comments.filter((c) => c.userId === meId).map((c) => c.id)
        );
        post.comments = post.comments
          // drop my own comments
          .filter((c) => c.userId !== meId)
          // drop replies whose parent was mine (now gone)
          .filter((c) => !c.parentId || !myCommentIds.has(c.parentId));
        // also wipe my reactions on remaining comments
        for (const c of post.comments) {
          if (c.reactions) {
            for (const emoji of Object.keys(c.reactions)) {
              c.reactions[emoji] = (c.reactions[emoji] || []).filter(
                (id) => id !== meId
              );
              if (c.reactions[emoji].length === 0) delete c.reactions[emoji];
            }
          }
        }
      }
    }

    // Strip me from other users' follow lists
    for (const other of db.users) {
      if (other.id === meId) continue;
      ensureSocial(other);
      other.followers = other.followers.filter((id) => id !== meId);
      other.following = other.following.filter((id) => id !== meId);
      other.pendingFollowers = other.pendingFollowers.filter((id) => id !== meId);
      other.pendingFollowing = other.pendingFollowing.filter((id) => id !== meId);
    }

    // Remove notifications to or from me
    if (Array.isArray(db.notifications)) {
      db.notifications = db.notifications.filter(
        (n) => n.userId !== meId && n.fromUserId !== meId
      );
    }

    // Finally remove the user
    db.users = db.users.filter((u) => u.id !== meId);

    await writeDb(db);

    logger.info(
      { userId: meId, name: user.name, postsDeleted, filesDeleted },
      "account deleted"
    );

    return res.status(200).json({
      message: "Account deleted",
      summary: { postsDeleted, filesDeleted },
    });
  } catch (error) {
    logger.error({ err: error, userId: req.user?.id }, "deleteMe failed");
    return res
      .status(500)
      .json({ message: "Could not delete account" });
  }
};

module.exports = {
  signup,
  signin,
  refresh,
  me,
  updateMe,
  deleteMe,
  getUserById,
  requestFollow,
  cancelFollowRequest,
  acceptFollow,
  rejectFollow,
};
