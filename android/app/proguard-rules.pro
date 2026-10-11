# Rules for shrinking the release build (R8).
#
# The pages in the app's web views call into Kotlin by name: CrundiNative on
# the server's page, CrundiScan on the scanner. R8 sees nothing in the Kotlin
# calling those methods and would rename or remove them, and the page would
# then find nothing there. Keep every method that is offered to a page.
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}
-keepattributes JavascriptInterface

# Line numbers in a crash report, and the real file names to go with them.
# The mapping file that turns the rest back into names is put into the bundle
# by the build, which is what the Play Console asks for.
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile
