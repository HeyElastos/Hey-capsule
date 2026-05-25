const { execFile } = require("child_process");
const { promisify } = require("util");
const fs = require("fs/promises");
const path = require("path");
const { randomUUID } = require("crypto");
const logger = require("./logger");

const execFileP = promisify(execFile);

// Codecs that play natively in all major browsers without OS-level codec
// support. Anything else gets transcoded to H.264 + AAC on upload.
const BROWSER_SAFE_VIDEO_CODECS = new Set(["h264"]);
const BROWSER_SAFE_AUDIO_CODECS = new Set(["aac", "mp3"]);

const probe = async (filePath) => {
  const { stdout } = await execFileP("ffprobe", [
    "-v", "error",
    "-show_entries", "stream=codec_type,codec_name",
    "-of", "json",
    filePath,
  ], { maxBuffer: 2 * 1024 * 1024 });
  const parsed = JSON.parse(stdout);
  const v = (parsed.streams || []).find((s) => s.codec_type === "video");
  const a = (parsed.streams || []).find((s) => s.codec_type === "audio");
  return {
    video: v?.codec_name || null,
    audio: a?.codec_name || null,
  };
};

const transcodeToH264 = async (inPath, outPath) => {
  await execFileP("ffmpeg", [
    "-y",
    "-i", inPath,
    "-c:v", "libx264",
    "-preset", "fast",
    "-crf", "23",
    "-pix_fmt", "yuv420p",
    "-c:a", "aac",
    "-b:a", "128k",
    "-movflags", "+faststart",
    outPath,
  ], { maxBuffer: 8 * 1024 * 1024 });
};

const remuxFaststart = async (inPath, outPath) => {
  // Stream-copy (no re-encode) and rewrite moov atom to the front. Fast and
  // lossless for files that are already H.264 + AAC.
  await execFileP("ffmpeg", [
    "-y",
    "-i", inPath,
    "-c", "copy",
    "-movflags", "+faststart",
    outPath,
  ], { maxBuffer: 8 * 1024 * 1024 });
};

// Takes a buffer from multer, writes it to a temp file, probes codecs, and
// returns the path of a browser-safe MP4 in `uploadsDir`. If the input is
// already H.264 + (AAC|MP3) we stream-copy with faststart; otherwise we
// transcode video to H.264 and audio to AAC. Cleans up the temp file.
const ensureBrowserSafeVideo = async (buffer, uploadsDir, originalExt) => {
  const tempPath = path.join(uploadsDir, `tmp-${randomUUID()}${originalExt}`);
  await fs.writeFile(tempPath, buffer);

  const finalName = `${randomUUID()}.mp4`;
  const finalPath = path.join(uploadsDir, finalName);

  try {
    const { video, audio } = await probe(tempPath);
    const vOk = BROWSER_SAFE_VIDEO_CODECS.has(video);
    const aOk = !audio || BROWSER_SAFE_AUDIO_CODECS.has(audio);

    if (vOk && aOk) {
      logger.debug({ video, audio }, "video already browser-safe, remuxing");
      await remuxFaststart(tempPath, finalPath);
    } else {
      logger.info({ video, audio }, "transcoding video to h264 + aac");
      await transcodeToH264(tempPath, finalPath);
    }
  } finally {
    fs.unlink(tempPath).catch(() => {});
  }

  return { fileName: finalName, url: `/uploads/${finalName}` };
};

module.exports = { ensureBrowserSafeVideo, probe };
