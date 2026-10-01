import hashlib

def hash_password(password, salt):
    """Derive a password digest with PBKDF2 and a per-user salt."""
    return hashlib.pbkdf2_hmac("sha256", password.encode(), salt, 100000)

def verify_password(password, salt, expected):
    """Check a supplied password against its stored digest."""
    import hmac
    return hmac.compare_digest(hash_password(password, salt), expected)

def password_needs_upgrade(iterations):
    """Upgrade password hashing when the stored work factor is low."""
    return iterations < 100000

def password_help_text():
    return "Choose a strong password"
