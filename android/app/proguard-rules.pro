# Rules for shrinking the release build (R8).
#
# Line numbers in a crash report, and the real file names to go with them.
# The mapping file that turns the rest back into names is put into the bundle
# by the build, which is what the Play Console asks for.
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile
