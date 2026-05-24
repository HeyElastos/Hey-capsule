import { useEffect, useRef, useState } from "react";
import { CloseIcon, PlusIcon } from "./icons";

const PHOTO_ACCEPTED = ["image/jpeg", "image/png", "image/webp", "image/avif", "image/heic", "image/heif", "image/gif"];
const VIDEO_ACCEPTED = ["video/mp4", "video/webm", "video/quicktime", "video/x-matroska"];

const ImageDropzone = ({ items, onChange, disabled = false, mode = "photo" }) => {
  const isVideo = mode === "video";
  const MAX_FILES = isVideo ? 1 : 12;
  const MAX_SIZE = isVideo ? 100 * 1024 * 1024 : 15 * 1024 * 1024;
  const ACCEPTED = isVideo ? VIDEO_ACCEPTED : PHOTO_ACCEPTED;
  const inputRef = useRef(null);
  const [dragOver, setDragOver] = useState(false);
  const [dragIndex, setDragIndex] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    return () => {
      for (const item of items) {
        URL.revokeObjectURL(item.preview);
      }
    };
  }, [items]);

  const addFiles = (fileList) => {
    setError(null);
    const incoming = Array.from(fileList);
    const accepted = [];
    for (const file of incoming) {
      if (!ACCEPTED.includes(file.type)) {
        setError(`Unsupported file: ${file.name}`);
        continue;
      }
      if (file.size > MAX_SIZE) {
        setError(`${file.name} is too large`);
        continue;
      }
      accepted.push({
        id: `${file.name}-${file.lastModified}-${file.size}-${Math.random()}`,
        file,
        preview: URL.createObjectURL(file),
      });
    }

    const remaining = MAX_FILES - items.length;
    if (accepted.length > remaining) {
      setError(`Maximum ${MAX_FILES} ${isVideo ? "video" : "files"} per post`);
    }
    const next = [...items, ...accepted.slice(0, remaining)];
    onChange(next);
  };

  const handleDrop = (event) => {
    event.preventDefault();
    setDragOver(false);
    if (disabled) return;
    if (event.dataTransfer.files?.length) {
      addFiles(event.dataTransfer.files);
    }
  };

  const remove = (id) => {
    onChange(items.filter((item) => item.id !== id));
  };

  const reorder = (from, to) => {
    if (from === to || from == null || to == null) return;
    const next = [...items];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    onChange(next);
  };

  const openPicker = () => !disabled && inputRef.current?.click();
  const canAddMore = items.length < MAX_FILES;
  const isEmpty = items.length === 0;

  const tileSize =
    items.length <= 1
      ? "h-48 w-48 sm:h-56 sm:w-56"
      : items.length <= 2
      ? "h-32 w-32"
      : items.length <= 4
      ? "h-24 w-24"
      : items.length <= 8
      ? "h-20 w-20"
      : "h-16 w-16";
  const tileRadius = items.length <= 2 ? "rounded-2xl" : "rounded-xl";
  const removeBtnSize =
    items.length <= 1 ? "h-7 w-7" : items.length <= 4 ? "h-6 w-6" : "h-5 w-5";
  const removeIconSize =
    items.length <= 1 ? "h-4 w-4" : items.length <= 4 ? "h-3.5 w-3.5" : "h-3 w-3";
  const plusIconSize =
    items.length <= 1 ? "h-8 w-8" : items.length <= 4 ? "h-6 w-6" : "h-5 w-5";

  return (
    <div className="space-y-3">
      <input
        ref={inputRef}
        type="file"
        accept={ACCEPTED.join(",")}
        multiple={!isVideo}
        onChange={(e) => {
          if (e.target.files?.length) addFiles(e.target.files);
          e.target.value = "";
        }}
        className="hidden"
      />

      {isEmpty ? (
        <div
          onDragOver={(e) => {
            e.preventDefault();
            if (!disabled) setDragOver(true);
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={handleDrop}
          onClick={openPicker}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if ((e.key === "Enter" || e.key === " ") && !disabled) {
              e.preventDefault();
              openPicker();
            }
          }}
          className={`relative flex cursor-pointer flex-col items-center justify-center rounded-[1.75rem] border-2 border-dashed p-8 text-center transition-all duration-300 ${
            dragOver
              ? "border-accent bg-accent/10 scale-[1.01]"
              : "border-surface-border bg-white/5 hover:bg-white/10"
          } ${disabled ? "pointer-events-none opacity-60" : ""}`}
        >
          <PlusIcon className="h-9 w-9 text-accent transition-transform duration-300" />
          <p className="mt-4 text-base font-semibold text-primary">
            {isVideo ? "Drop a video or click to upload" : "Drop images or click to upload"}
          </p>
          <p className="mt-1 text-sm text-muted">
            {isVideo
              ? "MP4, WebM, MOV · max 100MB"
              : `Up to ${MAX_FILES} photos · JPG, PNG, WebP, AVIF, HEIC · max 15MB each`}
          </p>
          {isVideo && (
            <p className="mt-1 text-xs text-muted">
              Played inline in the Spotlight feed.
            </p>
          )}
        </div>
      ) : (
        <div
          onDragOver={(e) => {
            e.preventDefault();
            if (!disabled && canAddMore) setDragOver(true);
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={handleDrop}
          className={`flex items-center gap-1 overflow-x-auto py-1 transition-all ${
            dragOver ? "rounded-2xl bg-accent/5 ring-2 ring-accent/40" : ""
          }`}
        >
          {items.map((item, i) => (
            <div
              key={item.id}
              draggable={!disabled}
              onDragStart={() => setDragIndex(i)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
                reorder(dragIndex, i);
                setDragIndex(null);
              }}
              className={`group relative flex-none animate-pop-in overflow-hidden frosted-card transition-all ${tileSize} ${tileRadius}`}
              style={{ animationDelay: `${i * 25}ms` }}
            >
              {item.file.type.startsWith("video/") ? (
                <video
                  src={item.preview}
                  className="absolute inset-0 h-full w-full object-cover"
                  muted
                  playsInline
                />
              ) : (
                <img
                  src={item.preview}
                  alt=""
                  className="absolute inset-0 h-full w-full object-cover"
                />
              )}
              {!disabled && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    remove(item.id);
                  }}
                  aria-label="Remove"
                  className={`unfrost absolute right-1 top-1 flex items-center justify-center rounded-md bg-black/60 text-white opacity-0 transition-opacity duration-150 group-hover:opacity-100 ${removeBtnSize}`}
                >
                  <CloseIcon className={removeIconSize} />
                </button>
              )}
            </div>
          ))}

          {canAddMore && !disabled && (
            <button
              type="button"
              onClick={openPicker}
              aria-label="Add more"
              className={`unfrost flex flex-none animate-pop-in items-center justify-center border-2 border-dashed border-surface-border bg-white/5 text-muted transition hover:border-accent hover:bg-white/10 hover:text-accent ${tileSize} ${tileRadius}`}
            >
              <PlusIcon className={plusIconSize} />
            </button>
          )}

          <span className="ml-2 flex-none text-xs text-muted">
            {items.length}/{MAX_FILES}
          </span>
        </div>
      )}

      {error && (
        <p className="animate-fade-in text-sm text-red-400">{error}</p>
      )}
    </div>
  );
};

export default ImageDropzone;
