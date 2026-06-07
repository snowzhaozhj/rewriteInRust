import { readFile, writeFile } from 'fs';
import { join, resolve } from 'path';

const data = readFile("test.txt", "utf8");
const full = join("a", "b");
