# Commit Gating

This project uses Git hooks to enforce code quality standards. All commits must pass the following checks:

## Pre-commit Checks

Before each commit, the following validations are automatically run:

1. **TypeScript Type Checking** (`npm run type-check`)
   - Ensures no TypeScript compilation errors
   - Validates type safety across the codebase

2. **ESLint Linting** (`npm run lint`)
   - Code style and quality checks
   - Enforces consistent formatting and best practices

3. **Build Validation** (`npm run build`)
   - Ensures the Next.js application builds successfully
   - Catches any build-time errors

## Setup

The commit gating is automatically set up when you install dependencies:

```bash
npm install
```

This installs Husky and configures the Git hooks.

## Manual Testing

You can run the checks manually at any time:

```bash
# Run all checks
npm run check

# Or run individually
npm run type-check
npm run lint
npm run build
```

## What Happens If Checks Fail

If any of the pre-commit checks fail:

1. The commit is **blocked**
2. You'll see error messages explaining what failed
3. You must fix the issues before committing

## Bypassing (Not Recommended)

In rare emergency situations, you can bypass the hooks:

```bash
git commit --no-verify -m "your commit message"
```

However, this should only be used for hotfixes and the issues should be resolved immediately after.

## Benefits

- **Prevents broken code** from entering the repository
- **Maintains code quality** standards
- **Catches issues early** before they reach CI/CD
- **Ensures consistency** across the team
