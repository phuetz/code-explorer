/**
 * Animated loading overlay with gradient orbs and optional stat counters.
 */

import { motion } from "framer-motion";

interface LoadingOrbsProps {
  label?: string;
  stats?: { label: string; value: number }[];
}

export function LoadingOrbs({ label, stats }: LoadingOrbsProps) {
  return (
    <div
      role="status"
      aria-label={label || "Loading"}
      aria-live="polite"
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        overflow: "hidden",
        background: "var(--bg-0)",
      }}
    >
      {/* Orb 1 — accent blue */}
      <motion.div
        style={{
          position: "absolute",
          width: 120,
          height: 120,
          borderRadius: "50%",
          background:
            "radial-gradient(circle, var(--accent) 0%, transparent 70%)",
          opacity: 0.3,
          filter: "blur(40px)",
        }}
        animate={{
          x: [0, 60, -40, 0],
          y: [0, -50, 30, 0],
          scale: [1, 1.3, 0.9, 1],
        }}
        transition={{ repeat: Infinity, duration: 4, ease: "easeInOut" }}
      />

      {/* Orb 2 — purple */}
      <motion.div
        style={{
          position: "absolute",
          width: 100,
          height: 100,
          borderRadius: "50%",
          background:
            "radial-gradient(circle, #bb9af7 0%, transparent 70%)",
          opacity: 0.25,
          filter: "blur(35px)",
        }}
        animate={{
          x: [0, -50, 40, 0],
          y: [0, 40, -30, 0],
          scale: [1, 0.8, 1.2, 1],
        }}
        transition={{
          repeat: Infinity,
          duration: 5,
          ease: "easeInOut",
          delay: 0.5,
        }}
      />

      {/* Orb 3 — cyan */}
      <motion.div
        style={{
          position: "absolute",
          width: 80,
          height: 80,
          borderRadius: "50%",
          background:
            "radial-gradient(circle, #7dcfff 0%, transparent 70%)",
          opacity: 0.2,
          filter: "blur(30px)",
        }}
        animate={{
          x: [0, 30, -60, 0],
          y: [0, -30, 50, 0],
          scale: [1, 1.1, 0.7, 1],
        }}
        transition={{
          repeat: Infinity,
          duration: 3.5,
          ease: "easeInOut",
          delay: 1,
        }}
      />

      {/* Label */}
      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        style={{
          position: "relative",
          zIndex: 1,
          color: "var(--text-1)",
          fontSize: 14,
          fontWeight: 500,
          letterSpacing: "0.02em",
        }}
      >
        {label || "Loading..."}
      </motion.div>

      {/* Animated progress dots */}
      <motion.div
        style={{
          display: "flex",
          gap: 6,
          marginTop: 12,
          position: "relative",
          zIndex: 1,
        }}
      >
        {[0, 1, 2].map((i) => (
          <motion.div
            key={i}
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: "var(--accent)",
            }}
            animate={{ opacity: [0.3, 1, 0.3], scale: [0.8, 1.2, 0.8] }}
            transition={{
              repeat: Infinity,
              duration: 1.2,
              delay: i * 0.2,
              ease: "easeInOut",
            }}
          />
        ))}
      </motion.div>

      {/* Stats counters */}
      {stats && stats.length > 0 && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.5 }}
          style={{
            display: "flex",
            gap: 24,
            marginTop: 20,
            position: "relative",
            zIndex: 1,
          }}
        >
          {stats.map((s) => (
            <div
              key={s.label}
              style={{
                textAlign: "center",
                color: "var(--text-2)",
                fontSize: 12,
              }}
            >
              <div
                style={{
                  fontSize: 20,
                  fontWeight: 600,
                  color: "var(--text-0)",
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {s.value.toLocaleString()}
              </div>
              <div>{s.label}</div>
            </div>
          ))}
        </motion.div>
      )}
    </div>
  );
}
