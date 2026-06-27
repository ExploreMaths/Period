const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

function getVersion() {
  try {
    const tag = execSync('git describe --tags --abbrev=0', {
      cwd: path.join(__dirname, '..'),
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'ignore'],
    }).trim();
    return tag.replace(/^v/, '');
  } catch (err) {
    console.warn('Unable to read git tag; keeping existing version.');
    return null;
  }
}

const version = getVersion();
if (!version) {
  process.exit(0);
}

const pkgPath = path.join(__dirname, 'package.json');
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
if (pkg.version === version) {
  console.log(`Extension version is already ${version}.`);
  process.exit(0);
}

pkg.version = version;
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');

const lockPath = path.join(__dirname, 'package-lock.json');
if (fs.existsSync(lockPath)) {
  const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
  lock.version = version;
  if (lock.packages && lock.packages['']) {
    lock.packages[''].version = version;
  }
  fs.writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
}

console.log(`Set extension version to ${version}.`);
