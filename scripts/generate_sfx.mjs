import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const sampleRate = 44_100;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputDirectory = resolve(root, "frontend/src/shared/audio");

function noise(index) {
  const value = Math.sin(index * 12.9898 + 78.233) * 43_758.5453;
  return (value - Math.floor(value)) * 2 - 1;
}

function writeWav(name, duration, sample) {
  const sampleCount = Math.floor(sampleRate * duration);
  const dataSize = sampleCount * 2;
  const buffer = Buffer.alloc(44 + dataSize);
  buffer.write("RIFF", 0);
  buffer.writeUInt32LE(36 + dataSize, 4);
  buffer.write("WAVE", 8);
  buffer.write("fmt ", 12);
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(1, 22);
  buffer.writeUInt32LE(sampleRate, 24);
  buffer.writeUInt32LE(sampleRate * 2, 28);
  buffer.writeUInt16LE(2, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write("data", 36);
  buffer.writeUInt32LE(dataSize, 40);

  for (let index = 0; index < sampleCount; index += 1) {
    const time = index / sampleRate;
    const progress = index / sampleCount;
    const value = Math.max(-1, Math.min(1, sample(time, progress, index)));
    buffer.writeInt16LE(Math.round(value * 32_767), 44 + index * 2);
  }
  writeFileSync(resolve(outputDirectory, `${name}.wav`), buffer);
}

mkdirSync(outputDirectory, { recursive: true });

writeWav("shot", 0.09, (time, progress, index) => {
  const frequency = 720 - progress * 430;
  const square = Math.sign(Math.sin(Math.PI * 2 * frequency * time));
  return (square * 0.55 + noise(index) * 0.2) * (1 - progress) ** 2;
});

writeWav("hit", 0.13, (time, progress, index) => {
  const square = Math.sign(Math.sin(Math.PI * 2 * (180 - progress * 70) * time));
  return (square * 0.3 + noise(index) * 0.55) * (1 - progress);
});

writeWav("dash", 0.16, (time, progress, index) => {
  const frequency = 180 + progress * 820;
  const tone = Math.sin(Math.PI * 2 * frequency * time);
  return (tone * 0.5 + noise(index) * 0.12) * Math.sin(Math.PI * progress);
});

writeWav("reload", 0.28, (time, progress) => {
  const clickOne = progress < 0.16 ? Math.sin(Math.PI * 2 * 960 * time) * (1 - progress / 0.16) : 0;
  const secondProgress = Math.max(0, (progress - 0.62) / 0.38);
  const clickTwo = progress > 0.62
    ? Math.sign(Math.sin(Math.PI * 2 * 680 * time)) * (1 - secondProgress)
    : 0;
  return clickOne * 0.45 + clickTwo * 0.35;
});

writeWav("countdown", 0.12, (time, progress) => (
  Math.sign(Math.sin(Math.PI * 2 * 440 * time)) * 0.3 * (1 - progress)
));

writeWav("round_start", 0.34, (time, progress) => {
  const note = progress < 0.45 ? 523.25 : 783.99;
  return Math.sign(Math.sin(Math.PI * 2 * note * time)) * 0.3 * (1 - progress * 0.45);
});

writeWav("round_end", 0.55, (time, progress) => {
  const note = progress < 0.34 ? 659.25 : progress < 0.68 ? 523.25 : 392.0;
  return Math.sign(Math.sin(Math.PI * 2 * note * time)) * 0.24 * (1 - progress);
});

console.log(`Generated sound effects in ${outputDirectory}`);
