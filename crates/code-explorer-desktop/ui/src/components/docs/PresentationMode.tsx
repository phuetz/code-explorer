import { useState, useEffect, useMemo } from "react";
import { X, ChevronLeft, ChevronRight, Play } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface PresentationModeProps {
  content: string;
  onExit: () => void;
  title?: string;
}

export function PresentationMode({ content, onExit, title }: PresentationModeProps) {
  // Split markdown into slides based on Marp `---` or `##` as fallback
  const slides = useMemo(() => {
    // Basic Marp-style parsing: split by `---` surrounded by blank lines
    let rawSlides = content.split(/\n---\n/g).map((s) => s.trim());

    // If there's only 1 slide, try splitting by H2 (##) to auto-chunk the document
    if (rawSlides.length <= 1) {
      const parts = content.split(/\n(?=##\s)/g).map(s => s.trim());
      if (parts.length > 1) {
        rawSlides = parts;
      }
    }
    
    // Remove empty slides and frontmatter blocks at the start if it exists
    return rawSlides.filter(s => s.length > 0 && !s.startsWith('marp: true'));
  }, [content]);

  const [currentSlide, setCurrentSlide] = useState(0);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onExit();
      if (e.key === "ArrowRight" || e.key === "Space" || e.key === "Enter") {
        setCurrentSlide((c) => Math.min(c + 1, slides.length - 1));
      }
      if (e.key === "ArrowLeft" || e.key === "Backspace") {
        setCurrentSlide((c) => Math.max(c - 1, 0));
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [slides.length, onExit]);

  return (
    <div
      className="fixed inset-0 z-50 flex flex-col fade-in"
      style={{
        background: "var(--bg-0)",
        color: "var(--text-0)",
      }}
    >
      {/* Header controls */}
      <div className="shrink-0 flex items-center justify-between px-6 py-4 absolute top-0 w-full z-10">
        <div className="flex items-center gap-2" style={{ color: "var(--text-3)", fontSize: 14 }}>
          <Play size={16} />
          {title || "Presentation"}
        </div>
        <button
          onClick={onExit}
          className="p-2 rounded-full hover:bg-[var(--bg-2)] transition-colors"
          style={{ color: "var(--text-2)" }}
          aria-label="Exit presentation"
        >
          <X size={20} />
        </button>
      </div>

      {/* Slide Content */}
      <div className="flex-1 flex items-center justify-center p-12 overflow-hidden relative">
        <div 
          className="w-full h-full max-w-5xl flex flex-col justify-center"
          style={{ fontSize: "1.5rem" }} // Bump up base font size for presentations
        >
          <div className="docs-prose presentation-prose slide-in-bottom">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {slides[currentSlide]}
            </ReactMarkdown>
          </div>
        </div>
      </div>

      {/* Footer controls & Progress */}
      <div className="shrink-0 flex flex-col absolute bottom-0 w-full z-10">
        <div className="flex items-center justify-center gap-6 p-4">
          <button
            onClick={() => setCurrentSlide((c) => Math.max(c - 1, 0))}
            disabled={currentSlide === 0}
            className="p-2 rounded-full hover:bg-[var(--bg-2)] disabled:opacity-30 transition-colors"
          >
            <ChevronLeft size={24} />
          </button>
          
          <span style={{ color: "var(--text-3)", fontSize: 14, fontVariantNumeric: "tabular-nums" }}>
            {currentSlide + 1} / {slides.length}
          </span>
          
          <button
            onClick={() => setCurrentSlide((c) => Math.min(c + 1, slides.length - 1))}
            disabled={currentSlide === slides.length - 1}
            className="p-2 rounded-full hover:bg-[var(--bg-2)] disabled:opacity-30 transition-colors"
          >
            <ChevronRight size={24} />
          </button>
        </div>
        
        {/* Progress bar */}
        <div className="h-1 w-full" style={{ background: "var(--bg-3)" }}>
          <div 
            className="h-full transition-all duration-300"
            style={{ 
              background: "var(--accent)", 
              width: `${((currentSlide + 1) / slides.length) * 100}%` 
            }}
          />
        </div>
      </div>
    </div>
  );
}