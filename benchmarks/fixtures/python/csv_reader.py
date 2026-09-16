import csv

def parse_csv_rows(stream):
    """Read CSV records as dictionaries keyed by the header."""
    return list(csv.DictReader(stream))

def valid_email_row(row):
    """Require an email address in an imported contact row."""
    return "@" in row.get("email", "")

def import_contacts(stream):
    """Import only CSV contacts with a valid email address."""
    return [row for row in parse_csv_rows(stream) if valid_email_row(row)]

def csv_export_filename():
    return "contacts-export.csv"
