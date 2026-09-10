use lru::LruCache;
use std::num::NonZeroUsize;
use tree_sitter::Tree;

/// LRU cache for parsed AST trees.
/// Keeps at most `capacity` trees in memory.
pub struct AstCache {
    cache: LruCache<String, Tree>,
}

impl AstCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(50).unwrap()),
            ),
        }
    }

    pub fn get(&mut self, file_path: &str) -> Option<&Tree> {
        self.cache.get(file_path)
    }

    pub fn put(&mut self, file_path: String, tree: Tree) {
        self.cache.put(file_path, tree);
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tree(code: &str) -> Tree {
        let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        parser.parse(code, None).unwrap()
    }

    #[test]
    fn test_cache_put_and_get() {
        let mut cache = AstCache::new(10);
        let tree = parse_tree("function hello() {}");

        cache.put("test.js".to_string(), tree);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        let cached = cache.get("test.js");
        assert!(cached.is_some());
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = AstCache::new(10);
        assert!(cache.get("nonexistent.js").is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = AstCache::new(2);
        let tree1 = parse_tree("var a = 1;");
        let tree2 = parse_tree("var b = 2;");
        let tree3 = parse_tree("var c = 3;");

        cache.put("a.js".to_string(), tree1);
        cache.put("b.js".to_string(), tree2);
        assert_eq!(cache.len(), 2);

        // Adding a third should evict the oldest (a.js)
        cache.put("c.js".to_string(), tree3);
        assert_eq!(cache.len(), 2);
        assert!(cache.get("a.js").is_none());
        assert!(cache.get("b.js").is_some());
        assert!(cache.get("c.js").is_some());
    }

    #[test]
    fn test_cache_zero_capacity_uses_default() {
        // NonZeroUsize::new(0) returns None, so fallback to 50
        let cache = AstCache::new(0);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }
}
