// Shared media-processing pipeline. Used by post.controller and
// chat.controller so both flows produce identical, browser-safe assets.
//
// Trust no client metadata. Inspect magic bytes (file-type), then transcode
// images → AVIF and videos → H.264 + AAC MP4 with faststart. Audio (voice
// notes) is pass-through: MediaRecorder already produces a browser-safe
// codec (webm/opus or ogg/opus), so we just verify the container.

const fs = require("fs/promises");
const { randomUUID } = require("crypto");
const path = require("path");
const sharp = require("sharp");
const fileType = require("file-type");
const { ensureBrowserSafeVideo } = require("./video");

const VIDEO_EXT_BY_MIME = {
  "video/mp4": ".mp4",
  "video/quicktime": ".mov",
  "video/webm": ".webm",
};

const IMAGE_MIMES_OK = new Set([
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/avif",
  "image/heic",
  "image/heif",
  "image/gif",
]);

const AUDIO_EXT_BY_MIME = {
  "audio/webm": ".webm",
  "audio/ogg":  ".ogg",
  "audio/mpeg": ".mp3",
  "audio/mp4":  ".m4a",
};
const AUDIO_MIMES_OK = new Set(Object.keys(AUDIO_EXT_BY_MIME));

// Multipart upload constraints — single source of truth so routes stay in sync.
const ALLOWED_MIMES = new Set([
  ...IMAGE_MIMES_OK,
  "video/mp4",
  "video/quicktime",
  "video/webm",
]);
const ALLOWED_AUDIO_MIMES = new Set(AUDIO_MIMES_OK);

const processImage = async (file, uploadsDir) => {
  const fileName = `${randomUUID()}.avif`;
  const outputPath = path.join(uploadsDir, fileName);
  const { width, height } = await sharp(file.buffer)
    .rotate()
    .resize({ width: 1600, withoutEnlargement: true })
    .avif({ quality: 65 })
    .toFile(outputPath);
  return { url: `/uploads/${fileName}`, type: "photo", width, height };
};

const processVideo = async (file, uploadsDir, detectedMime) => {
  const ext = VIDEO_EXT_BY_MIME[detectedMime];
  if (!ext) throw new Error("Unsupported video type");
  const { url } = await ensureBrowserSafeVideo(file.buffer, uploadsDir, ext);
  return { url, type: "video", mime: "video/mp4" };
};

const processFile = async (file, uploadsDir) => {
  const detected = await fileType.fromBuffer(file.buffer);
  if (!detected) throw new Error("Could not detect file type");
  const realMime = detected.mime;
  if (VIDEO_EXT_BY_MIME[realMime]) {
    return processVideo(file, uploadsDir, realMime);
  }
  if (IMAGE_MIMES_OK.has(realMime)) {
    return processImage(file, uploadsDir);
  }
  throw new Error(`Disallowed file type: ${realMime}`);
};

// Voice notes are accepted by mime + magic-byte cross-check. WebM is the
// same container as video WebM, so we can't dispatch by magic bytes alone —
// callers must use a dedicated voice endpoint that asserts kind=audio,
// then this validates that the container at least matches (webm/ogg) and
// writes the bytes through unchanged. Modern browsers play opus directly.
const processAudio = async (file, uploadsDir, declaredMime) => {
  if (!ALLOWED_AUDIO_MIMES.has(declaredMime)) {
    throw new Error(`Disallowed audio type: ${declaredMime}`);
  }
  const detected = await fileType.fromBuffer(file.buffer);
  if (!detected) throw new Error("Could not detect audio container");
  // WebM and Ogg containers can carry audio OR video. We accept video-tagged
  // containers from MediaRecorder because Chrome reports video/webm even
  // for audio-only opus tracks. The browser still decodes them as audio.
  const ok = AUDIO_MIMES_OK.has(detected.mime) ||
             detected.mime === "video/webm" ||
             detected.mime === "video/ogg" ||
             detected.mime === "application/ogg";
  if (!ok) throw new Error(`Audio container mismatch: ${detected.mime}`);

  const ext = AUDIO_EXT_BY_MIME[declaredMime];
  const fileName = `${randomUUID()}${ext}`;
  const outputPath = path.join(uploadsDir, fileName);
  await fs.writeFile(outputPath, file.buffer);
  return {
    url: `/uploads/${fileName}`,
    type: "voice",
    mime: declaredMime,
  };
};

module.exports = {
  processFile,
  processAudio,
  ALLOWED_MIMES,
  ALLOWED_AUDIO_MIMES,
  VIDEO_EXT_BY_MIME,
  IMAGE_MIMES_OK,
};
