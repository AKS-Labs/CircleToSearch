# Image Search Architecture

## Overview

This app captures a screenshot, lets the user draw a region, crops the bitmap, and uploads it to multiple reverse image search providers. The results page is displayed inside a `WebView` with JavaScript and DOM storage enabled.

## Flow

- Capture screenshot and display in `OverlayActivity`.
- User draws a region; the stroke’s bounding box becomes the selection.
- Crop the screenshot to the selection using `ImageUtils.cropBitmap`.
- Upload the cropped bitmap via `ImageSearchUploader` to the chosen provider.
- If the chosen provider fails, try other providers as fallback.
- Load the returned results URL in `WebView`.

## Engines

- Google Lens: `POST https://lens.google.com/upload` field `encoded_image`. Response may be HTML with JS challenge. Parser detects `location.replace` or noscript fallback link.
- Bing Visual Search: `POST https://www.bing.com/images/visualsearch/upload` field `image`. Redirect URL found in `Location` header.
- Yandex Images: `POST https://yandex.com/images-search` field `upfile`. Redirect URL found in `Location` header.
- TinEye: `POST https://tineye.com/search` field `image`. Redirect via `Location` header.

## Error Handling

- Each uploader logs status codes and parsing paths.
- On upload failure or missing redirect, fallback tries other engines automatically.
- `WebView` is configured with `javaScriptEnabled` and `domStorageEnabled` to handle provider pages.

## Logging

- Cropping rect and cropped size logged.
- Upload response codes, redirect parsing steps, and fallback decisions logged.

## Performance

- Images resized to max side 1024px before upload; JPEG quality 85.
- Upload timeouts set to 15s.

## Compatibility

- UI and interactions preserved.
- Engine list expanded with TinEye; existing selections continue to work.

