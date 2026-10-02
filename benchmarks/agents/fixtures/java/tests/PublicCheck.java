import example.paths.PathPolicy;

public class PublicCheck {
    public static void main(String[] args) {
        PathPolicy policy = new PathPolicy() {
            public String pathHeading() { return "Paths"; }
        };
        if (!policy.matchesDirectoryPrefix("src", "src")) throw new AssertionError();
        if (!policy.matchesDirectoryPrefix("src/main", "src")) throw new AssertionError();
        if (policy.allowsRelativePath("../private")) throw new AssertionError();
    }
}
