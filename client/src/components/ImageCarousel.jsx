import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeftIcon, ChevronRightIcon } from "./icons";

const clampRatio = (r) => {
  if (!r || !Number.isFinite(r) || r <= 0) return 1;
  return Math.max(0.6, Math.min(1.91, r));
};

const ImageCarousel = ({ images = [] }) => {
  const containerRef = useRef(null);
  const dragRef = useRef({
    active: false,
    startX: 0,
    scrollStart: 0,
    moved: false,
  });
  const [index, setIndex] = useState(0);
  const [loaded, setLoaded] = useState(() => images.map(() => false));

  useEffect(() => {
    setLoaded(images.map(() => false));
  }, [images]);

  useEffect(() => {
    const node = containerRef.current;
    if (!node) return;

    let raf;
    const handleScroll = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const width = node.clientWidth;
        if (!width) return;
        const next = Math.round(node.scrollLeft / width);
        setIndex((current) => (current === next ? current : next));
      });
    };

    node.addEventListener("scroll", handleScroll, { passive: true });
    return () => {
      cancelAnimationFrame(raf);
      node.removeEventListener("scroll", handleScroll);
    };
  }, []);

  useEffect(() => {
    if (images.length <= 1) return;
    const node = containerRef.current;
    if (!node) return;

    const handleMove = (event) => {
      if (!dragRef.current.active) return;
      event.preventDefault();
      const dx = dragRef.current.startX - event.clientX;
      if (Math.abs(dx) > 4) dragRef.current.moved = true;
      node.scrollLeft = dragRef.current.scrollStart + dx;
    };

    const handleUp = () => {
      if (!dragRef.current.active) return;
      const moved = dragRef.current.moved;
      dragRef.current.active = false;
      dragRef.current.moved = false;
      node.style.cursor = "";
      node.style.scrollBehavior = "";
      document.body.style.userSelect = "";

      const width = node.clientWidth;
      if (width) {
        const nearest = Math.round(node.scrollLeft / width);
        node.scrollTo({
          left: nearest * width,
          behavior: "smooth",
        });
      }

      // restore snap after the smooth scroll begins
      requestAnimationFrame(() => {
        node.style.scrollSnapType = "";
      });

      if (moved) {
        const blockClick = (e) => {
          e.stopPropagation();
          e.preventDefault();
        };
        window.addEventListener("click", blockClick, { capture: true, once: true });
        setTimeout(() => {
          window.removeEventListener("click", blockClick, { capture: true });
        }, 50);
      }
    };

    document.addEventListener("mousemove", handleMove);
    document.addEventListener("mouseup", handleUp);
    return () => {
      document.removeEventListener("mousemove", handleMove);
      document.removeEventListener("mouseup", handleUp);
    };
  }, [images.length]);

  const handleMouseDown = (event) => {
    if (images.length <= 1) return;
    if (event.button !== 0) return;
    const node = containerRef.current;
    if (!node) return;
    dragRef.current = {
      active: true,
      moved: false,
      startX: event.clientX,
      scrollStart: node.scrollLeft,
    };
    node.style.cursor = "grabbing";
    node.style.scrollSnapType = "none";
    node.style.scrollBehavior = "auto";
    document.body.style.userSelect = "none";
  };

  const scrollTo = (i) => {
    const node = containerRef.current;
    if (!node) return;
    node.scrollTo({ left: i * node.clientWidth, behavior: "smooth" });
  };

  const markLoaded = (i) =>
    setLoaded((arr) => {
      if (arr[i]) return arr;
      const next = [...arr];
      next[i] = true;
      return next;
    });

  const containerAspect = useMemo(() => {
    if (!images.length) return 1;
    const first = images[0];
    if (first.width && first.height) {
      return clampRatio(first.width / first.height);
    }
    return 1;
  }, [images]);

  if (images.length === 0) return null;

  return (
    <div className="relative overflow-hidden rounded-[1.5rem]">
      <div
        ref={containerRef}
        onMouseDown={handleMouseDown}
        onDragStart={(e) => e.preventDefault()}
        className="scroll-snap-x flex overflow-x-auto"
        style={images.length > 1 ? { cursor: "grab" } : undefined}
      >
        {images.map((img, i) => (
          <div
            key={img.url + i}
            className="relative w-full flex-none overflow-hidden"
            style={{ aspectRatio: containerAspect }}
          >
            {!loaded[i] && <div className="absolute inset-0 image-skeleton" />}
            {img.type === "video" ? (
              <>
                <video
                  src={img.url}
                  muted
                  playsInline
                  preload="metadata"
                  aria-hidden="true"
                  className="absolute inset-0 h-full w-full object-cover scale-110 blur-[60px]"
                />
                <video
                  src={img.url}
                  controls
                  playsInline
                  preload="metadata"
                  onLoadedData={() => markLoaded(i)}
                  className={`relative h-full w-full object-contain transition-opacity duration-500 ${
                    loaded[i] ? "opacity-100" : "opacity-0"
                  }`}
                />
              </>
            ) : (
              <>
                <img
                  src={img.url}
                  alt=""
                  aria-hidden="true"
                  loading={i === 0 ? "eager" : "lazy"}
                  className="absolute inset-0 h-full w-full object-cover scale-110 blur-[60px]"
                />
                <img
                  src={img.url}
                  alt=""
                  loading={i === 0 ? "eager" : "lazy"}
                  onLoad={() => markLoaded(i)}
                  className={`relative h-full w-full object-contain transition-opacity duration-500 ${
                    loaded[i] ? "opacity-100" : "opacity-0"
                  }`}
                />
              </>
            )}
          </div>
        ))}
      </div>

      {images.length > 1 && (
        <>
          {index > 0 && (
            <button
              type="button"
              onClick={() => scrollTo(index - 1)}
              aria-label="Previous image"
              className="absolute left-3 top-1/2 z-10 -translate-y-1/2 unfrost flex h-9 w-9 items-center justify-center rounded-full bg-black/40 text-white backdrop-blur-md transition hover:bg-black/60"
            >
              <ChevronLeftIcon className="h-5 w-5" />
            </button>
          )}
          {index < images.length - 1 && (
            <button
              type="button"
              onClick={() => scrollTo(index + 1)}
              aria-label="Next image"
              className="absolute right-3 top-1/2 z-10 -translate-y-1/2 unfrost flex h-9 w-9 items-center justify-center rounded-full bg-black/40 text-white backdrop-blur-md transition hover:bg-black/60"
            >
              <ChevronRightIcon className="h-5 w-5" />
            </button>
          )}

          <div className="absolute right-3 top-3 z-10 rounded-full bg-black/55 px-2.5 py-1 text-xs font-medium text-white backdrop-blur-md">
            {index + 1} / {images.length}
          </div>

          <div className="absolute bottom-3 left-1/2 z-10 -translate-x-1/2 flex gap-1.5">
            {images.map((_, i) => (
              <button
                key={i}
                type="button"
                onClick={() => scrollTo(i)}
                aria-label={`Go to image ${i + 1}`}
                className={`unfrost h-1.5 rounded-full transition-all duration-300 ${
                  i === index ? "w-6 bg-white" : "w-1.5 bg-white/60"
                }`}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
};

export default ImageCarousel;
