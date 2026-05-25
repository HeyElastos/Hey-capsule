import { useEffect, useState } from "react";

// Image that swaps to `fallback` if the source fails to load — including the
// tricky case where the browser served a broken image from cache and marked
// `complete: true` before our onError handler could attach. Reset on src change.
export const SafeImage = ({ src, fallback = null, onError, ...rest }) => {
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFailed(false);
  }, [src]);

  const handleRef = (node) => {
    if (!node) return;
    // complete=true with naturalWidth=0 means the cached load failed before
    // we got to attach onError. naturalWidth>0 means it loaded fine.
    if (node.complete && node.naturalWidth === 0 && node.src) {
      setFailed(true);
    }
  };

  const handleError = (e) => {
    setFailed(true);
    onError?.(e);
  };

  if (!src || failed) return fallback;
  return <img ref={handleRef} src={src} onError={handleError} {...rest} />;
};

// Same idea for <video>: fall back if the source fails. Useful for thumbnail
// previews on profile/clip grids where a 404'd video would otherwise show an
// ugly "video unavailable" native UI.
export const SafeVideo = ({ src, fallback = null, onError, ...rest }) => {
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFailed(false);
  }, [src]);

  const handleRef = (node) => {
    if (!node) return;
    if (node.error) setFailed(true);
  };

  const handleError = (e) => {
    setFailed(true);
    onError?.(e);
  };

  if (!src || failed) return fallback;
  return <video ref={handleRef} src={src} onError={handleError} {...rest} />;
};
