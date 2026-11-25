# Rust Admin Server

**High-performance RBAC/ABAC authentication & authorization service**

Modern admin server with hybrid access control (RBAC + ABAC) built with Rust, Axum, and SQLite.

---

## 🎯 Key Features

### Access Control
- **RBAC (Role-Based Access Control)** - Role and permission management
- **ABAC (Attribute-Based Access Control)** - Policy-based fine-grained control
- **Hybrid Mode** - Combine RBAC and ABAC for maximum flexibility

### Authentication & Security
- JWT-based authentication (access + refresh tokens)
- Password hashing with bcrypt
- Permission-based authorization
- Audit logging for all access decisions

### Management Interface
- Modern responsive web UI (Tailwind CSS)
- Real-time dashboard with metrics
- User, role, and permission management
- Policy management interface
- Access log viewer

### Resilience
- **Circuit Breaker** - Protects the system from failures in downstream services
- **Request Timeouts** - Prevents long-running requests from degrading system performance

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Web UI (Tera)                      │
│              Tailwind CSS + Vanilla JS                  │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────┐
│                   Axum Web Server                       │
│  ┌──────────────┬──────────────┬─────────────────┐    │
│  │   Auth API   │  Admin API   │  Access Control │    │
│  │  (JWT/Login) │ (CRUD Users) │   (RBAC/ABAC)   │    │
│  └──────┬───────┴──────┬───────┴────────┬────────┘    │
└─────────┼──────────────┼────────────────┼─────────────┘
          │              │                │
┌─────────▼──────────────▼────────────────▼─────────────┐
│              Service Layer (Business Logic)            │
│   UserService │ RoleService │ PermissionService        │
│   AuthService │ PolicyService │ AccessControlService   │
└─────────┬──────────────────────────────────────────────┘
          │
┌─────────▼──────────────────────────────────────────────┐
│          Repository Layer (Data Access)                │
│   UserRepo │ RoleRepo │ PermissionRepo │ PolicyRepo    │
└─────────┬──────────────────────────────────────────────┘
          │
┌─────────▼──────────────────────────────────────────────┐
│                  SQLite Database                        │
│  admin_user │ roles │ permissions │ abac_policy        │
│  access_evaluation_log │ history                        │
└────────────────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### Prerequisites
- **Rust** 1.70+ (stable)
- **SQLite** 3.35+
- **sqlx-cli** (`cargo install sqlx-cli`)

### Installation

1. **Clone the repository**
   ```bash
   git clone <repository-url>
   cd rust-admin-server-dev
   ```

2. **Setup environment**
   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```

3. **Run database migrations**
   ```bash
   sqlx migrate run
   # Note: This applies the consolidated schema migration.
   ```

4. **Build and run**
   ```bash
   cargo run --release
   ```

5. **Access the application**
   - Web UI: `http://localhost:3000`
   - API: `http://localhost:3000/api`
   - Default credentials: `admin` / (set in first run)

---

## 📚 API Endpoints

### Authentication
```
POST   /api/auth/login          # Login with username/password
POST   /api/auth/register       # Register new user
POST   /api/auth/refresh        # Refresh access token
GET    /api/auth/me             # Get current user info
POST   /api/auth/logout         # Logout (invalidate token)
```

### User Management
```
GET    /api/users               # List users (with pagination)
GET    /api/users/detail?id=x   # Get user by ID
POST   /api/users               # Create new user
PUT    /api/users/update?id=x   # Update user
DELETE /api/users/delete?id=x   # Delete user
```

### Role Management
```
GET    /api/roles               # List roles
GET    /api/roles/detail?id=x   # Get role by ID
POST   /api/roles               # Create new role
PUT    /api/roles/update?id=x   # Update role
DELETE /api/roles/delete?id=x   # Delete role
```

### Permission Management
```
GET    /api/permissions               # List permissions
GET    /api/permissions/detail?id=x   # Get permission by ID
POST   /api/permissions               # Create permission
PUT    /api/permissions/update?id=x   # Update permission
DELETE /api/permissions/delete?id=x   # Delete permission
```

### Policy Management (ABAC)
```
GET    /api/access-control/policies                # List policies
GET    /api/access-control/policies/detail?id=x    # Get policy
POST   /api/access-control/policies                # Create policy
PUT    /api/access-control/policies/update?id=x    # Update policy
DELETE /api/access-control/policies/delete?id=x    # Delete policy
POST   /api/access-control/policies/activate?id=x    # Activate policy
POST   /api/access-control/policies/deactivate?id=x  # Deactivate policy
POST   /api/access-control/policies/toggle?id=x      # Toggle policy status
POST   /api/access-control/policies/validate         # Validate policy JSON
POST   /api/access-control/policies/test             # Test policy
```

### Dashboard & Monitoring
```
GET    /api/dashboard           # Dashboard metrics
GET    /api/history             # Audit log history
GET    /api/access-logs         # Access evaluation logs
```

**API Documentation**: See [docs/api/](docs/api/) for detailed API specs.

---

## 🛠️ Technology Stack

| Category | Technology |
|----------|-----------|
| **Language** | Rust 1.70+ |
| **Web Framework** | Axum 0.7 |
| **Database** | SQLite 3.35+ with SQLx 0.8 |
| **Template Engine** | Tera 1.20 |
| **Authentication** | JWT (jsonwebtoken 9.3) |
| **Password Hashing** | Argon2 |
| **Logging** | tracing + tracing-subscriber |
| **Validation** | validator 0.20 |
| **Frontend** | Tailwind CSS 3.x + Vanilla JS |
| **Resilience** | Custom Circuit Breaker |

---

## 📁 Project Structure

```
rust-admin-server-dev/
├── src/
│   ├── main.rs                  # Application entry point
│   ├── config/                  # Configuration modules
│   │   ├── container.rs         # Dependency injection
│   │   └── security.rs          # Security config
│   ├── handlers/
│   │   ├── api/                 # API endpoints
│   │   │   ├── auth.rs          # Authentication
│   │   │   ├── user.rs          # User management
│   │   │   ├── role.rs          # Role management
│   │   │   ├── permission.rs    # Permission management
│   │   │   └── access_control/  # ABAC policies
│   │   └── views/               # HTML page handlers
│   ├── services/                # Business logic layer
│   │   ├── auth.rs
│   │   ├── user.rs
│   │   ├── role.rs
│   │   ├── permission.rs
│   │   └── access_control/      # RBAC/ABAC engines
│   ├── repositories/            # Data access layer
│   │   ├── auth.rs
│   │   ├── user.rs
│   │   ├── role.rs
│   │   └── permission.rs
│   ├── models/                  # Data models
│   │   ├── entities/            # Database entities
│   │   └── dto/                 # Data transfer objects
│   ├── middleware/              # Middleware (auth, etc.)
│   ├── errors.rs                # Error handling
│   └── auth/                    # Auth utilities (JWT, password)
├── migrations/                  # Database migrations
├── static/                      # Frontend assets
│   ├── js/                      # JavaScript files
│   ├── css/                     # Stylesheets
│   └── templates/               # Tera templates
└── docs/                        # Documentation
    ├── architecture.md          # Architecture details
    ├── operations-guide.md      # Operations guide
    └── api/                     # API documentation
```

---

## 🔒 Security

- **Password Hashing**: bcrypt with secure defaults (DEFAULT_COST)
- **JWT Tokens**: HS256 with configurable expiry
- **Permission Validation**: Every request is authenticated and authorized
- **SQL Injection**: Protected by SQLx compile-time checks
- **Audit Logging**: All authentication and authorization events logged



---

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run with logging
RUST_LOG=debug cargo test

# Run specific test
cargo test test_name
```

---

## 📖 Documentation

- **[Architecture Guide](docs/architecture.md)** - System architecture and design
- **[Operations Guide](docs/operations-guide.md)** - Deployment and operations
- **[API Documentation](docs/api/)** - Detailed API specifications

