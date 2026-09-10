//! Route extractors — extract HTTP route definitions from framework-specific code.
//!
//! Currently implemented:
//! - **C#/ASP.NET MVC 5 & Web API**: controller detection, action extraction,
//!   route attribute parsing, DbContext/Entity discovery, Razor view analysis.

pub mod csharp;
pub mod edmx;
