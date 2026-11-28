/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PhotoCamera
import androidx.compose.material.icons.filled.PhotoLibrary
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.akslabs.circletosearch.R

/**
 * Where the next image comes from: Camera first, then Gallery.
 *
 * Deliberately does NOT show a thumbnail grid. A grid of the user's photos needs
 * READ_MEDIA_IMAGES (and on Android 14+ a second partial-access permission), which
 * is a heavy ask for one button — and the system photo picker behind the Gallery
 * tile already shows every photo without granting this app anything. So the Gallery
 * tile is entirely permission-free.
 *
 * The Camera tile hands off to `ACTION_IMAGE_CAPTURE`, which itself needs no
 * permission — but this APK declares CAMERA (inherited from the bundled translator
 * module), and the platform then rejects that action unless the grant is held, so
 * the capture path may ask once.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ImageSourceSheet(
    onDismiss: () -> Unit,
    onOpenCamera: () -> Unit,
    onOpenGallery: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 20.dp, end = 20.dp, bottom = 28.dp),
        ) {
            Text(
                text = stringResource(R.string.picker_title),
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = stringResource(R.string.picker_subtitle),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(modifier = Modifier.height(16.dp))

            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                SourceTile(
                    label = stringResource(R.string.label_camera),
                    caption = stringResource(R.string.picker_camera_caption),
                    icon = { tint ->
                        Icon(
                            imageVector = Icons.Default.PhotoCamera,
                            contentDescription = null,
                            tint = tint,
                            modifier = Modifier.size(30.dp),
                        )
                    },
                    onClick = onOpenCamera,
                    modifier = Modifier.weight(1f),
                )
                SourceTile(
                    label = stringResource(R.string.label_gallery),
                    caption = stringResource(R.string.picker_gallery_caption),
                    icon = { tint ->
                        Icon(
                            imageVector = Icons.Default.PhotoLibrary,
                            contentDescription = null,
                            tint = tint,
                            modifier = Modifier.size(30.dp),
                        )
                    },
                    onClick = onOpenGallery,
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

@Composable
private fun SourceTile(
    label: String,
    caption: String,
    icon: @Composable (androidx.compose.ui.graphics.Color) -> Unit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .aspectRatio(1f)
            .clip(RoundedCornerShape(20.dp))
            .background(MaterialTheme.colorScheme.surfaceContainerHighest)
            .clickable(onClick = onClick)
            .padding(12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            // Themed tint rather than a fixed colour, so it follows Material You.
            icon(MaterialTheme.colorScheme.primary)
            Spacer(modifier = Modifier.height(10.dp))
            Text(
                text = label,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(modifier = Modifier.height(2.dp))
            Text(
                text = caption,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
