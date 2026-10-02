/** Preserve local images and embedded raster captures without remote loading. */
export function offlineMarkdownImageUrl(url: string): string {
  if (/^data:image\/(?:png|jpeg|gif|webp);base64,[a-z0-9+/]+={0,2}$/i.test(url)) {
    return url;
  }
  // Backslashes can become authority separators in browser URLs. SVG data
  // images are excluded because their content may contain active resources.
  if (!url || /[\s\\]/.test(url) || /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(url)) {
    return '';
  }
  return url;
}
