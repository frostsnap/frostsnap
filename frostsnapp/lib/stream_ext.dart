import 'package:rxdart/rxdart.dart';

extension StreamToBehaviorSubjectExtension<T> on Stream<T> {
  BehaviorSubject<T> toBehaviorSubject({T? seedValue}) {
    // Initialize the BehaviorSubject with a seed value if provided
    final BehaviorSubject<T> subject = seedValue != null
        ? BehaviorSubject.seeded(seedValue)
        : BehaviorSubject<T>();

    listen(
      (data) => subject.add(data),
      onError: (error) => subject.addError(error),
      onDone: () => subject.close(),
    );

    return subject;
  }
}
