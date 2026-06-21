import { useCallback, useEffect, useRef, useState } from 'react';

type HorizontalAlign = 'start' | 'end';

interface Options {
  align?: HorizontalAlign;
  gutter?: number;
  maxWidth?: number;
  minWidth?: number;
  offset?: number;
  onClose: () => void;
  open: boolean;
}

interface PopoverPosition {
  left: number;
  maxHeight: number;
  top: number;
  width: number;
}

export function useAnchoredPopover<TAnchor extends HTMLElement, TPopover extends HTMLElement>({
  align = 'end',
  gutter = 12,
  maxWidth = 360,
  minWidth = 288,
  offset = 8,
  onClose,
  open,
}: Options) {
  const anchorRef = useRef<TAnchor | null>(null);
  const popoverRef = useRef<TPopover | null>(null);
  const [position, setPosition] = useState<PopoverPosition | null>(null);

  const updatePosition = useCallback(() => {
    const anchor = anchorRef.current;
    if (!anchor) return;

    const rect = anchor.getBoundingClientRect();
    const availableWidth = Math.max(160, window.innerWidth - gutter * 2);
    const width = Math.min(Math.max(minWidth, rect.width), maxWidth, availableWidth);
    const preferredLeft = align === 'start' ? rect.left : rect.right - width;
    const left = clamp(preferredLeft, gutter, window.innerWidth - width - gutter);
    const top = Math.min(rect.bottom + offset, window.innerHeight - gutter);
    const maxHeight = Math.max(160, window.innerHeight - top - gutter);
    setPosition({ left, maxHeight, top, width });
  }, [align, gutter, maxWidth, minWidth, offset]);

  useEffect(() => {
    if (!open) return undefined;

    updatePosition();

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (anchorRef.current?.contains(target) || popoverRef.current?.contains(target)) return;
      onClose();
    };
    const onViewportChange = () => updatePosition();

    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('resize', onViewportChange);
    window.addEventListener('scroll', onViewportChange, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('resize', onViewportChange);
      window.removeEventListener('scroll', onViewportChange, true);
    };
  }, [onClose, open, updatePosition]);

  return {
    anchorRef,
    popoverRef,
    position,
    updatePosition,
  };
}

function clamp(value: number, min: number, max: number): number {
  if (max < min) return min;
  return Math.min(Math.max(value, min), max);
}
