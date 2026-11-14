package dev.zgui.android;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;
import java.net.URLConnection;

/** Serves files in the shared directory to other applications. The provider is not exported,
 * so another application reads or writes a file only through a URI grant.
 */
public final class ZguiFiles extends ContentProvider {
    static File dir(Context c) { return new File(c.getFilesDir(), "zgui-shared"); }

    static Uri uri(Context c, String name) {
        return Uri.parse("content://" + c.getPackageName() + ".zgui.files/" + Uri.encode(name));
    }

    private File file(Uri uri) throws FileNotFoundException {
        String name = uri.getLastPathSegment();
        if (name == null || name.contains("/") || name.equals("..") || name.equals("."))
            throw new FileNotFoundException("Invalid file name");
        return new File(dir(getContext()), name);
    }

    @Override public boolean onCreate() { return true; }

    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.parseMode(mode));
    }

    @Override public String getType(Uri uri) {
        String type = URLConnection.guessContentTypeFromName(uri.getLastPathSegment());
        return type == null ? "application/octet-stream" : type;
    }

    @Override public Cursor query(Uri uri, String[] columns, String selection, String[] args, String order) {
        File file;
        try { file = file(uri); } catch (FileNotFoundException invalid) { return null; }
        if (columns == null) columns = new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};
        MatrixCursor result = new MatrixCursor(columns);
        Object[] row = new Object[columns.length];
        for (int i = 0; i < columns.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) row[i] = file.getName();
            else if (OpenableColumns.SIZE.equals(columns[i])) row[i] = file.length();
        }
        result.addRow(row);
        return result;
    }

    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException(); }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException(); }
    @Override public int delete(Uri uri, String selection, String[] args) { throw new UnsupportedOperationException(); }
}
