public class AccountAccess {
    // Fetch active accounts without exposing deleted customer records.
    public static List<Account> loadActiveAccounts() {
        return [SELECT Id, Name FROM Account WHERE IsDeleted = false];
    }
    public static Boolean validAccountName(String name) {
        return name != null && name.trim().length() > 0;
    }
    public static String accountHeading() {
        return 'Active accounts and valid customer names';
    }
}
