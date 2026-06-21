/**
 * SymbolAutocomplete — Multi-column autocomplete for symbol search.
 *
 * Features:
 * - Debounced FTS search against the knowledge graph
 * - Multi-column dropdown: Type badge | Name | File path | Lines
 * - Keyboard navigation (arrows, Enter, Escape)
 * - Color-coded type badges (Class, Controller, Method, etc.)
 */

import { useState, useRef, useEffect, useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { commands, type SearchResult } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";

// ─── Label colors ───────────────────────────────────────────────

const LABEL_COLORS: Record<string, string> = {
  Class: "var(--accent)",
  Controller: "#bb9af7",
  Service: "var(--green)",
  Method: "var(--amber)",
  Function: "var(--amber)",
  Interface: "#2ac3de",
  Struct: "var(--accent)",
  Constructor: "var(--amber)",
  View: "var(--rose)",
  DbEntity: "#ff9e64",
  DbContext: "#ff9e64",
  Enum: "#2ac3de",
  Module: "#565f89",
  File: "#565f89",
  Property: "#73daca",
  Variable: "#73daca",
};

const LABEL_ABBREV: Record<string, string> = {
  Class: "CLS",
  Controller: "CTR",
  ControllerAction: "ACT",
  Service: "SVC",
  Method: "MTD",
  Function: "FUN",
  Interface: "IFC",
  Struct: "STR",
  Constructor: "CON",
  View: "VUE",
  DbEntity: "ENT",
  DbContext: "CTX",
  Enum: "ENM",
  Module: "MOD",
  File: "FIL",
  Property: "PRP",
  Variable: "VAR",
  Repository: "REP",
};

function labelColor(label: string): string {
  return LABEL_COLORS[label] || "var(--text-3)";
}

function labelAbbrev(label: string): string {
  return LABEL_ABBREV[label] || label.slice(0, 3).toUpperCase();
}

// Shorten file path for display
function shortPath(filePath: string, maxLen = 45): string {
  if (!filePath) return "";
  const normalized = filePath.replace(/\\/g, "/");
  if (normalized.length <= maxLen) return normalized;
  const parts = normalized.split("/");
  if (parts.length <= 2) return normalized;
  // Keep first dir + ... + last 2 parts
  return parts[0] + "/.../" + parts.slice(-2).join("/");
}

// ─── Props ──────────────────────────────────────────────────────

interface SymbolAutocompleteProps {
  value: string;
  onChange: (value: string) => void;
  onSelect: (result: SearchResult) => void;
  placeholder?: string;
  autoFocus?: boolean;
}

// ─── Component ──────────────────────────────────────────────────

export function SymbolAutocomplete({
  value,
  onChange,
  onSelect,
  placeholder = "Search symbols...",
  autoFocus = false,
}: SymbolAutocompleteProps) {
  const { t } = useI18n();
  const [isOpen, setIsOpen] = useState(false);
  const [highlightIndex, setHighlightIndex] = useState(0);
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Debounce the search query (200ms). All setState calls happen inside the
  // timeout callback so the effect itself doesn't synchronously mutate state.
  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    const trimmed = value.trim();
    const shortValue = trimmed.length < 2;
    debounceRef.current = setTimeout(() => {
      if (shortValue) {
        setDebouncedQuery("");
        setIsOpen(false);
      } else {
        setDebouncedQuery(trimmed);
        setIsOpen(true);
        setHighlightIndex(0);
      }
    }, shortValue ? 0 : 200);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [value]);

  // FTS search
  const { data: results } = useQuery({
    queryKey: ["symbol-autocomplete", debouncedQuery],
    queryFn: () => commands.searchSymbols(debouncedQuery, 15),
    enabled: debouncedQuery.length >= 2,
    staleTime: 30_000,
  });

  const items = results || [];

  // Scroll highlighted item into view
  useEffect(() => {
    if (listRef.current && isOpen) {
      const el = listRef.current.children[highlightIndex] as HTMLElement;
      if (el) el.scrollIntoView({ block: "nearest" });
    }
  }, [highlightIndex, isOpen]);

  // Close on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        inputRef.current &&
        !inputRef.current.contains(e.target as Node) &&
        listRef.current &&
        !listRef.current.contains(e.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const handleSelect = useCallback(
    (item: SearchResult) => {
      onChange(item.name);
      onSelect(item);
      setIsOpen(false);
    },
    [onChange, onSelect],
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!isOpen || items.length === 0) {
      if (e.key === "ArrowDown" && items.length > 0) {
        setIsOpen(true);
        e.preventDefault();
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setHighlightIndex((i) => Math.min(i + 1, items.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlightIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        e.preventDefault();
        if (items[highlightIndex]) {
          handleSelect(items[highlightIndex]);
        }
        break;
      case "Escape":
        e.preventDefault();
        setIsOpen(false);
        break;
      case "Tab":
        if (items[highlightIndex]) {
          handleSelect(items[highlightIndex]);
        }
        setIsOpen(false);
        break;
    }
  };

  return (
    <div style={{ position: "relative", flex: 1 }}>
      {/* Search icon */}
      <Search
        size={14}
        style={{
          position: "absolute",
          left: 12,
          top: "50%",
          transform: "translateY(-50%)",
          color: "var(--text-3)",
          pointerEvents: "none",
        }}
      />

      {/* Input */}
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onFocus={() => {
          if (items.length > 0 && value.trim().length >= 2) setIsOpen(true);
        }}
        placeholder={placeholder}
        autoFocus={autoFocus}
        autoComplete="off"
        spellCheck={false}
        style={{
          width: "100%",
          padding: "8px 12px 8px 32px",
          borderRadius: isOpen && items.length > 0 ? "var(--radius-md) var(--radius-md) 0 0" : "var(--radius-md)",
          border: "1px solid var(--surface-border)",
          background: "var(--bg-2)",
          color: "var(--text-0)",
          fontSize: 13,
        }}
      />

      {/* Dropdown */}
      {isOpen && items.length > 0 && (
        <div
          ref={listRef}
          role="listbox"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            right: 0,
            maxHeight: 340,
            overflowY: "auto",
            background: "var(--bg-1)",
            border: "1px solid var(--surface-border)",
            borderTop: "none",
            borderRadius: "0 0 var(--radius-md) var(--radius-md)",
            zIndex: 100,
            boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
          }}
        >
          {/* Column headers */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "52px 1fr 1.2fr 60px",
              gap: 8,
              padding: "6px 10px",
              borderBottom: "1px solid var(--surface-border)",
              fontSize: 10,
              fontWeight: 600,
              textTransform: "uppercase",
              letterSpacing: "0.06em",
              color: "var(--text-4)",
            }}
          >
            <span>{t("symbol.columnType")}</span>
            <span>{t("symbol.columnName")}</span>
            <span>{t("symbol.columnFile")}</span>
            <span style={{ textAlign: "right" }}>{t("symbol.columnLines")}</span>
          </div>

          {/* Results */}
          {items.map((item, idx) => (
            <div
              key={item.nodeId}
              role="option"
              aria-selected={idx === highlightIndex}
              onClick={() => handleSelect(item)}
              onMouseEnter={() => setHighlightIndex(idx)}
              style={{
                display: "grid",
                gridTemplateColumns: "52px 1fr 1.2fr 60px",
                gap: 8,
                padding: "7px 10px",
                cursor: "pointer",
                background: idx === highlightIndex ? "var(--accent-subtle)" : "transparent",
                borderLeft: idx === highlightIndex ? "2px solid var(--accent)" : "2px solid transparent",
                transition: "background 0.1s",
              }}
            >
              {/* Type badge */}
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 9,
                  fontWeight: 700,
                  fontFamily: "var(--font-mono)",
                  letterSpacing: "0.04em",
                  padding: "2px 6px",
                  borderRadius: 4,
                  color: labelColor(item.label),
                  background: `${labelColor(item.label)}18`,
                  border: `1px solid ${labelColor(item.label)}30`,
                  whiteSpace: "nowrap",
                }}
              >
                {labelAbbrev(item.label)}
              </span>

              {/* Name */}
              <span
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  color: "var(--text-0)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {item.name}
              </span>

              {/* File path */}
              <span
                style={{
                  fontSize: 11,
                  color: "var(--text-3)",
                  fontFamily: "var(--font-mono)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
                title={item.filePath}
              >
                {shortPath(item.filePath)}
              </span>

              {/* Lines */}
              <span
                style={{
                  fontSize: 11,
                  color: "var(--text-4)",
                  fontFamily: "var(--font-mono)",
                  textAlign: "right",
                }}
              >
                {item.startLine && item.endLine
                  ? `${item.startLine}-${item.endLine}`
                  : item.startLine
                    ? `L${item.startLine}`
                    : ""}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
