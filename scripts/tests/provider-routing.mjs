import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chooseProvider } from '../../src/services/providers.ts';
test('Native Bilibili identifiers, links and share text retain native parsing', () => {
  for (const input of [
    'BV1xx411c7mD',
    'av12345',
    'ep123',
    'ss123',
    'md123',
    'au123',
    'am123',
    'cv123',
    'https://www.bilibili.com/video/BV1xx411c7mD',
    'https://b23.tv/example',
    '分享影片 https://www.bilibili.com/video/BV1xx411c7mD 更多文字',
    'bilibili.com/video/BV1xx411c7mD',
  ])
    assert.equal(chooseProvider(input), 'bilibili', input);
});
test('Explicit native content types retain numeric ID support', () => {
  assert.equal(chooseProvider('12345', true), 'bilibili');
});
test('Other platforms and an explicit YouTube search use the independent provider', () => {
  for (const input of [
    'https://youtube.com/watch?v=abc',
    'https://youtube.com/playlist?list=abc',
    'https://vimeo.com/123',
    'ytsearch20:music',
    'https://example.com/video.mp4',
  ])
    assert.equal(chooseProvider(input), 'ytdlp', input);
});
