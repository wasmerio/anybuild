"""Typed service configurations used by provider configs."""

def mysql():
    """Request a managed MySQL database."""
    return struct(name = "database", engine = "mysql")

def postgres():
    """Request a managed PostgreSQL database."""
    return struct(name = "database", engine = "postgres")
