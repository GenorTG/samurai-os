// Lines of a running task's output, capped so a long update cannot grow without bound.
#pragma once

#include <QAbstractListModel>
#include <QByteArray>
#include <QStringList>

class LogModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ count NOTIFY countChanged)

public:
    static constexpr int kMaxLines = 5000;

    using QAbstractListModel::QAbstractListModel;

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    int count() const { return static_cast<int>(m_lines.size()); }

    /// Raw process output: split into lines, terminal control sequences removed, and of a line
    /// redrawn with `\r` only the last state kept.
    void feed(const QByteArray &bytes);
    /// Emit a trailing line that had no newline.
    void flush();
    void append(const QString &line);
    void clear();
    Q_INVOKABLE QString text() const { return m_lines.join(QLatin1Char('\n')); }

signals:
    void countChanged();

private:
    void appendLines(const QStringList &lines);
    static QString clean(QByteArray line);

    QStringList m_lines;
    QByteArray m_partial;
};
