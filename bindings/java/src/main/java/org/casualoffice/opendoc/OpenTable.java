// SPDX-License-Identifier: Apache-2.0
package org.casualoffice.opendoc;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** A table, with POI's {@code XWPFTable} naming. */
public final class OpenTable {
    private final OpenDocument document;
    private final String id;
    /**
     * Cells per row, read once and kept up to date by {@link #createRow()}: a
     * POI loop ({@code getRow(r).getCell(c).setText(...)}) must not re-project
     * the whole table per cell, which made it O(rows^2).
     */
    private List<Integer> shape;

    OpenTable(OpenDocument document, String id) {
        this.document = document;
        this.id = id;
    }

    public String getId() {
        return id;
    }

    @SuppressWarnings("unchecked")
    List<Object> rows() {
        return (List<Object>) document.tableView(id).get("rows");
    }

    @SuppressWarnings("unchecked")
    private List<Integer> shape() {
        if (shape == null) {
            shape = new ArrayList<>();
            for (Object row : rows()) {
                shape.add(((List<Object>) ((Map<String, Object>) row).get("cells")).size());
            }
        }
        return shape;
    }

    /** The number of rows ({@code getNumberOfRows}). */
    public int getNumberOfRows() {
        shape = null; // another call may have added or removed a row
        return shape().size();
    }

    /** A row by index ({@code getRow}). */
    public Row getRow(int index) {
        if (index < 0 || index >= shape().size()) {
            shape = null;
            if (index < 0 || index >= shape().size()) {
                throw new IndexOutOfBoundsException("row " + index);
            }
        }
        return new Row(index);
    }

    /** Appends a row copying the last row's layout ({@code createRow}). */
    public Row createRow() {
        List<Integer> cells = shape();
        document.edit(OpenDocument.cmd("op", "addRow", "table", id));
        // The engine copies the last row's layout.
        cells.add(cells.isEmpty() ? 0 : cells.get(cells.size() - 1));
        return new Row(cells.size() - 1);
    }

    /** The cells' text, tab-separated, one line per row ({@code getText}). */
    public String getText() {
        StringBuilder out = new StringBuilder();
        for (int r = 0; r < getNumberOfRows(); r++) {
            Row row = getRow(r);
            for (int c = 0; c < row.cellCount(); c++) {
                if (c > 0) {
                    out.append('\t');
                }
                out.append(row.getCell(c).getText());
            }
            out.append('\n');
        }
        return out.toString();
    }

    /** A table row ({@code XWPFTableRow}). */
    public final class Row {
        private final int index;

        Row(int index) {
            this.index = index;
        }

        int cellCount() {
            return shape().get(index);
        }

        /** A cell by index ({@code getCell}). */
        public Cell getCell(int column) {
            if (column < 0 || column >= cellCount()) {
                throw new IndexOutOfBoundsException("cell " + column);
            }
            return new Cell(index, column);
        }

        public int getTableCellsCount() {
            return cellCount();
        }
    }

    /** A table cell ({@code XWPFTableCell}). */
    public final class Cell {
        private final int row;
        private final int column;

        Cell(int row, int column) {
            this.row = row;
            this.column = column;
        }

        /** Replaces the cell's text ({@code setText}). */
        public void setText(String text) {
            document.edit(OpenDocument.cmd("op", "setCellText", "table", id, "row", row, "column", column, "text", text));
        }

        /** The cell's text ({@code getText}). */
        @SuppressWarnings("unchecked")
        public String getText() {
            Map<String, Object> r = (Map<String, Object>) rows().get(row);
            Map<String, Object> cell = (Map<String, Object>) ((List<Object>) r.get("cells")).get(column);
            StringBuilder out = new StringBuilder();
            for (Object block : (List<Object>) cell.get("blocks")) {
                Object text = ((Map<String, Object>) block).get("text");
                if (out.length() > 0) {
                    out.append('\n');
                }
                out.append(text == null ? "" : text);
            }
            return out.toString();
        }
    }
}
