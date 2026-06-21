/**
 * Shared animation primitives built on framer-motion.
 * Import these wrappers instead of using motion.div directly.
 */

import {
  motion,
  AnimatePresence,
  useSpring,
  useMotionValue,
  useTransform,
} from "framer-motion";
import {
  type ReactNode,
  type CSSProperties,
  useEffect,
  useRef,
} from "react";

// ─── Page Transition ────────────────────────────────────────────────

interface AnimatedPageProps {
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}

export function AnimatedPage({ children, className, style }: AnimatedPageProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.2, ease: "easeOut" }}
      className={className}
      style={{ ...style, height: "100%", width: "100%" }}
    >
      {children}
    </motion.div>
  );
}

// ─── Card with Hover Lift ───────────────────────────────────────────

interface AnimatedCardProps {
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
  delay?: number;
  onClick?: () => void;
}

export function AnimatedCard({
  children,
  className,
  style,
  delay = 0,
  onClick,
}: AnimatedCardProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay, duration: 0.25, ease: "easeOut" }}
      whileHover={{ y: -2, transition: { duration: 0.15 } }}
      whileTap={{ scale: 0.99 }}
      className={className}
      style={style}
      onClick={onClick}
    >
      {children}
    </motion.div>
  );
}

// ─── Modal with Backdrop ────────────────────────────────────────────

interface AnimatedModalProps {
  isOpen: boolean;
  onClose: () => void;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}

export function AnimatedModal({
  isOpen,
  onClose,
  children,
  className,
  style,
}: AnimatedModalProps) {
  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          style={{ background: "rgba(0,0,0,0.6)" }}
          onClick={(e) => e.target === e.currentTarget && onClose()}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.97, y: 4 }}
            transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
            className={className}
            style={style}
          >
            {children}
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ─── Animated Counter ───────────────────────────────────────────────

interface AnimatedCounterProps {
  value: number;
  duration?: number;
  className?: string;
  style?: CSSProperties;
}

export function AnimatedCounter({
  value,
  duration = 0.8,
  className,
  style,
}: AnimatedCounterProps) {
  const ref = useRef<HTMLSpanElement>(null);
  const motionValue = useMotionValue(0);
  const spring = useSpring(motionValue, { duration: duration * 1000 });
  const display = useTransform(spring, (v) =>
    Math.round(v).toLocaleString()
  );

  useEffect(() => {
    motionValue.set(value);
  }, [value, motionValue]);

  useEffect(() => {
    const unsubscribe = display.on("change", (v) => {
      if (ref.current) ref.current.textContent = v;
    });
    return unsubscribe;
  }, [display]);

  return <span ref={ref} className={className} style={style}>0</span>;
}

// ─── Stagger Container / Item ───────────────────────────────────────

const staggerContainerVariants = {
  hidden: {},
  show: {
    transition: { staggerChildren: 0.05 },
  },
};

const staggerItemVariants = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: 0.25, ease: "easeOut" as const } },
};

interface StaggerProps {
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}

export function StaggerContainer({ children, className, style }: StaggerProps) {
  return (
    <motion.div
      variants={staggerContainerVariants}
      initial="hidden"
      animate="show"
      className={className}
      style={style}
    >
      {children}
    </motion.div>
  );
}

export function StaggerItem({ children, className, style }: StaggerProps) {
  return (
    <motion.div variants={staggerItemVariants} className={className} style={style}>
      {children}
    </motion.div>
  );
}

// ─── Skeleton Placeholders ──────────────────────────────────────────

interface SkeletonProps {
  width?: string;
  height?: string;
  rounded?: string;
  className?: string;
}

export function SkeletonLine({
  width = "100%",
  height = "14px",
  rounded = "6px",
  className,
}: SkeletonProps) {
  return (
    <div
      className={`shimmer ${className ?? ""}`}
      style={{
        width,
        height,
        borderRadius: rounded,
        background: "var(--bg-3)",
      }}
    />
  );
}

export function SkeletonBlock({
  width = "100%",
  height = "80px",
  rounded = "12px",
  className,
}: SkeletonProps) {
  return (
    <div
      className={`shimmer ${className ?? ""}`}
      style={{
        width,
        height,
        borderRadius: rounded,
        background: "var(--bg-3)",
      }}
    />
  );
}

