//! The public product page, the original dashboard and their static assets. See
//! `docs/spec/site-v1.md`.
mod assets;
mod routes;
pub(crate) use routes::routes;

#[cfg(test)]
mod tests;
