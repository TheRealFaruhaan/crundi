package com.crundi.app

import android.content.Context
import android.content.Intent
import android.net.Uri

/**
 * Where this build gets its updates: GitHub releases.
 *
 * This file exists only in the `github` build. The `play` build has its own
 * Updater and none of this, so a copy from the Play Store never asks GitHub
 * for anything and cannot be pointed at an APK.
 */
object Updater {
    const val LABEL = "Get the latest version"
    const val WHERE = "the releases page on GitHub"

    fun open(ctx: Context) {
        ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/TheRealFaruhaan/crundi/releases")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}
