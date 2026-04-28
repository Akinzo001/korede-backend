// The port layer is where repository/service traits will live.
//
// In hexagonal architecture, the domain should depend on traits
// instead of depending directly on infrastructure like PostgreSQL.
//
// Example for later:
// pub trait HospitalRepository { ... }
