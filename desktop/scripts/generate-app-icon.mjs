import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const desktopRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const projectRoot = resolve(desktopRoot, "..");
const iconDir = resolve(desktopRoot, "src-tauri", "icons");
const appAssetDir = resolve(desktopRoot, "src", "app", "assets");
const siteAssetDir = resolve(projectRoot, "site", "assets");
const sourcePath = resolve(iconDir, "app-icon-source.png");

if (!existsSync(sourcePath)) {
  throw new Error(`Missing Imagen source icon: ${sourcePath}`);
}

mkdirSync(iconDir, { recursive: true });
mkdirSync(appAssetDir, { recursive: true });
mkdirSync(siteAssetDir, { recursive: true });

function icoFromPngs(images) {
  const headerSize = 6;
  const entrySize = 16;
  const directorySize = headerSize + images.length * entrySize;
  const totalSize = directorySize + images.reduce((sum, image) => sum + image.buffer.length, 0);
  const ico = Buffer.alloc(totalSize);

  ico.writeUInt16LE(0, 0);
  ico.writeUInt16LE(1, 2);
  ico.writeUInt16LE(images.length, 4);

  let imageOffset = directorySize;
  images.forEach((image, index) => {
    const entryOffset = headerSize + index * entrySize;
    ico.writeUInt8(image.size >= 256 ? 0 : image.size, entryOffset);
    ico.writeUInt8(image.size >= 256 ? 0 : image.size, entryOffset + 1);
    ico.writeUInt8(0, entryOffset + 2);
    ico.writeUInt8(0, entryOffset + 3);
    ico.writeUInt16LE(1, entryOffset + 4);
    ico.writeUInt16LE(32, entryOffset + 6);
    ico.writeUInt32LE(image.buffer.length, entryOffset + 8);
    ico.writeUInt32LE(imageOffset, entryOffset + 12);
    image.buffer.copy(ico, imageOffset);
    imageOffset += image.buffer.length;
  });

  return ico;
}

async function renderPng(size) {
  return sharp(sourcePath)
    .resize(size, size, { fit: "cover", position: "centre", kernel: "lanczos3" })
    .ensureAlpha()
    .png({ compressionLevel: 9, adaptiveFiltering: true })
    .toBuffer();
}

const sizes = [16, 24, 32, 48, 64, 128, 256];
const iconPngs = await Promise.all(
  sizes.map(async (size) => ({
    size,
    buffer: await renderPng(size),
  })),
);

writeFileSync(resolve(iconDir, "icon.png"), await renderPng(256));
writeFileSync(resolve(iconDir, "icon.ico"), icoFromPngs(iconPngs));
writeFileSync(resolve(appAssetDir, "app-icon.png"), await renderPng(256));
writeFileSync(resolve(projectRoot, "site", "favicon.png"), await renderPng(64));
writeFileSync(resolve(siteAssetDir, "app-icon.png"), await renderPng(512));

console.log(`Synced Imagen app icon from ${sourcePath}`);
