module.exports = {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "scope-empty": [2, "never"],
    "scope-enum": [
      2,
      "always",
      [
        "cli",
        "stack",
        "restack",
        "providers",
        "github",
        "gitlab",
        "tests",
        "docs",
        "deps",
        "build",
        "ci",
        "release",
        "tooling",
      ],
    ],
  },
};
