import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SyntaxCodeBlock } from './SyntaxCodeBlock';

describe('SyntaxCodeBlock', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
      configurable: true,
    });
  });

  it('shows the resolved language and copies the raw code', () => {
    const code = 'const value: number = 1;';
    const { container } = render(<SyntaxCodeBlock language="typescript" code={code} />);

    expect(screen.getByText('typescript')).toBeTruthy();
    expect(container.textContent).toContain('const');

    fireEvent.click(screen.getByRole('button', { name: /copier le bloc de code/i }));

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(code);
  });

  it('falls back to a readable plain block for unknown languages', () => {
    const code = 'plain text payload';
    const { container } = render(<SyntaxCodeBlock language="madeup" code={code} />);

    expect(screen.getByText('madeup')).toBeTruthy();
    expect(container.querySelector('pre')?.textContent).toBe(code);
  });
});
