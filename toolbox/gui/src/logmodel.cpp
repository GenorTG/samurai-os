#include "logmodel.hpp"

#include <QRegularExpression>

namespace {
constexpr qsizetype kMaxLineBytes = 4000;
}

int LogModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : count();
}

QVariant LogModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= count() || role != Qt::DisplayRole) {
        return {};
    }
    return m_lines.at(index.row());
}

QHash<int, QByteArray> LogModel::roleNames() const
{
    return {{Qt::DisplayRole, "line"}};
}

QString LogModel::clean(QByteArray line)
{
    while (line.endsWith('\r')) {
        line.chop(1);
    }
    const qsizetype redraw = line.lastIndexOf('\r');
    if (redraw >= 0) {
        line = line.mid(redraw + 1);
    }
    static const QRegularExpression escapes(QStringLiteral(
        "\x1b(\\[[0-9;?]*[ -/]*[@-~]|\\][^\x07\x1b]*(\x07|\x1b\\\\)|[()][0-9A-Za-z]|[@-_])"));
    QString text = QString::fromUtf8(line.left(kMaxLineBytes));
    text.remove(escapes);
    return text;
}

void LogModel::feed(const QByteArray &bytes)
{
    m_partial += bytes;
    QStringList lines;
    qsizetype start = 0;
    for (qsizetype nl = m_partial.indexOf('\n'); nl >= 0; nl = m_partial.indexOf('\n', start)) {
        lines << clean(m_partial.mid(start, nl - start));
        start = nl + 1;
    }
    m_partial.remove(0, start);
    if (m_partial.size() > kMaxLineBytes) {
        lines << clean(m_partial);
        m_partial.clear();
    }
    appendLines(lines);
}

void LogModel::flush()
{
    if (!m_partial.isEmpty()) {
        appendLines({clean(m_partial)});
        m_partial.clear();
    }
}

void LogModel::append(const QString &line)
{
    flush();
    appendLines({line});
}

void LogModel::appendLines(const QStringList &lines)
{
    if (lines.isEmpty()) {
        return;
    }
    const auto incoming = lines.size() > kMaxLines ? lines.mid(lines.size() - kMaxLines) : lines;
    const int overflow = count() + static_cast<int>(incoming.size()) - kMaxLines;
    if (overflow > 0) {
        beginRemoveRows({}, 0, overflow - 1);
        m_lines.remove(0, overflow);
        endRemoveRows();
    }
    beginInsertRows({}, count(), count() + static_cast<int>(incoming.size()) - 1);
    m_lines.append(incoming);
    endInsertRows();
    emit countChanged();
}

void LogModel::clear()
{
    m_partial.clear();
    beginResetModel();
    m_lines.clear();
    endResetModel();
    emit countChanged();
}
